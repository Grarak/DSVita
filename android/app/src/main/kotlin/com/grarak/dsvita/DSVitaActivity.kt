package com.grarak.dsvita

import android.annotation.SuppressLint
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import android.graphics.Color
import android.graphics.Typeface
import android.os.Bundle
import android.os.Handler
import android.os.Looper
import android.view.Gravity
import android.view.KeyEvent
import android.view.MotionEvent
import android.view.Surface
import android.view.SurfaceHolder
import android.view.SurfaceView
import android.view.View
import android.view.ViewGroup
import android.util.Log
import android.view.WindowManager
import android.widget.FrameLayout
import android.widget.LinearLayout
import android.widget.TextView
import androidx.appcompat.app.AppCompatActivity
import androidx.core.content.ContextCompat
import androidx.core.view.WindowCompat
import androidx.core.view.WindowInsetsCompat
import androidx.core.view.WindowInsetsControllerCompat
import androidx.recyclerview.widget.LinearLayoutManager
import androidx.recyclerview.widget.RecyclerView
import com.google.android.material.appbar.MaterialToolbar
import com.google.android.material.card.MaterialCardView
import com.google.android.material.dialog.MaterialAlertDialogBuilder
import com.google.android.material.materialswitch.MaterialSwitch
import org.json.JSONArray
import org.json.JSONObject
import java.io.File

// All UI lives here (rom browser, settings, pause dialog, on-screen controls); the
// native side renders emulator frames into the SurfaceView and blocks in
// present_ui/present_pause until this Activity calls nativeLaunch/nativePauseChoice.
// Settings render generically from JSON serialized out of the Rust definitions — the
// native side stays the single source of truth. See presenter/android.rs.
class DSVitaActivity : AppCompatActivity(), SurfaceHolder.Callback {
    companion object {
        init {
            System.loadLibrary("dsvita")
        }

        // input::Keycode discriminants (src/core/input.rs)
        const val DS_A = 0
        const val DS_B = 1
        const val DS_SELECT = 2
        const val DS_START = 3
        const val DS_RIGHT = 4
        const val DS_LEFT = 5
        const val DS_UP = 6
        const val DS_DOWN = 7
        const val DS_TRIGGER_R = 8
        const val DS_TRIGGER_L = 9
        const val DS_X = 10
        const val DS_Y = 11

        const val PAUSE_RESUME = 0
        const val PAUSE_QUIT = 2

        var initialized = false
    }

    private external fun nativeInit(storagePath: String)
    private external fun nativeSurfaceCreated(surface: Surface)
    private external fun nativeSurfaceDestroyed()
    private external fun nativeLaunch(romPath: String)
    private external fun nativeTouch(x: Int, y: Int, down: Boolean)
    private external fun nativeKey(dsKey: Int, down: Boolean)
    private external fun nativePause()
    private external fun nativePauseChoice(choice: Int)
    private external fun nativeResume()
    private external fun nativeGetGameSettings(romPath: String): String
    private external fun nativeSetGameSetting(romPath: String, idx: Int, value: Int)
    private external fun nativeGetRuntimeSettings(): String
    private external fun nativeSetRuntimeSetting(idx: Int, value: Int)
    private external fun nativeGetDebugText(): String
    private external fun nativeDebugCmd(cmd: String): String

    private lateinit var root: FrameLayout
    private lateinit var surfaceView: SurfaceView
    private lateinit var browser: View
    private lateinit var settingsScreen: SettingsScreen
    private lateinit var controls: OnScreenControls
    private lateinit var debugText: TextView
    private val roms = ArrayList<File>()
    private var inGame = false
    private var paused = false

    // Debug command bridge (release builds reply "err"):
    //   adb shell am broadcast -a com.grarak.dsvita.DBG --es cmd "touch 128 96; press a"
    // ';'-separated commands, grammar in presenter/dbg_cmds.rs; replies land in logcat
    // under DSVitaDbg.
    private val debugReceiver = object : BroadcastReceiver() {
        override fun onReceive(context: Context, intent: Intent) {
            val line = intent.getStringExtra("cmd") ?: return
            for (cmd in line.split(';')) {
                Log.i("DSVitaDbg", "${cmd.trim()} -> ${nativeDebugCmd(cmd.trim())}")
            }
        }
    }

    // Debug-stats OSD: the native render loop publishes the same line the desktop OSD
    // draws (empty = disabled); poll it into the TextView while a game runs.
    private val uiHandler = Handler(Looper.getMainLooper())
    private val debugTextPoll = object : Runnable {
        override fun run() {
            if (!inGame) {
                debugText.visibility = View.GONE
                return
            }
            val text = nativeGetDebugText()
            if (text.isEmpty()) {
                debugText.visibility = View.GONE
            } else {
                debugText.visibility = View.VISIBLE
                debugText.text = text
            }
            uiHandler.postDelayed(this, 500)
        }
    }

    private fun dp(v: Int) = (v * resources.displayMetrics.density).toInt()

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        WindowCompat.setDecorFitsSystemWindows(window, true)
        window.addFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)

        // No opaque root background: the SurfaceView composites behind the window through
        // its hole-punch, and the GL side clears the letterbox border itself.
        root = FrameLayout(this)

        surfaceView = SurfaceView(this)
        surfaceView.holder.addCallback(this)
        surfaceView.setOnTouchListener { _, event -> onSurfaceTouch(event) }
        root.addView(surfaceView, matchParent())

        controls = OnScreenControls(this) { key, down -> if (inGame) nativeKey(key, down) }
        controls.visibility = View.GONE
        root.addView(controls, matchParent())

        debugText = TextView(this)
        debugText.setTextColor(Color.WHITE)
        debugText.setShadowLayer(4f, 0f, 0f, Color.BLACK)
        debugText.textSize = 12f
        debugText.typeface = Typeface.MONOSPACE
        debugText.gravity = Gravity.CENTER_HORIZONTAL
        debugText.visibility = View.GONE
        // Top-center: clear of the L/R shoulder buttons in the screen corners.
        val debugTextLp = FrameLayout.LayoutParams(ViewGroup.LayoutParams.WRAP_CONTENT, ViewGroup.LayoutParams.WRAP_CONTENT)
        debugTextLp.gravity = Gravity.TOP or Gravity.CENTER_HORIZONTAL
        debugTextLp.topMargin = dp(6)
        root.addView(debugText, debugTextLp)

        browser = buildBrowser()
        root.addView(browser, matchParent())

        settingsScreen = SettingsScreen(this)
        settingsScreen.view.visibility = View.GONE
        root.addView(settingsScreen.view, matchParent())

        setContentView(root)

        if (!initialized) {
            initialized = true
            nativeInit(storageDir().absolutePath)
        }
        refreshRomList()

        ContextCompat.registerReceiver(this, debugReceiver, IntentFilter("com.grarak.dsvita.DBG"), ContextCompat.RECEIVER_EXPORTED)

        // Direct-launch intent (adb / file manager): skip the browser and boot the rom.
        romFromIntent(intent)?.let { launchRom(it) }
    }

    override fun onDestroy() {
        super.onDestroy()
        unregisterReceiver(debugReceiver)
    }

    // External storage can be unavailable — an unmounted emulated volume on some
    // Waydroid/emulator setups makes getExternalFilesDir return null. Fall back to the
    // always-present internal files dir so the app still runs; roms load fine via an
    // absolute-path launch intent regardless of where the browser scans.
    private fun storageDir(): File = getExternalFilesDir(null) ?: filesDir

    // A launcher intent may target a specific rom two ways:
    //   am start -n com.grarak.dsvita/.DSVitaActivity --es rom <abs-path-or-filename>
    //   am start -a android.intent.action.VIEW -d file://<abs-path>   (.nds VIEW filter)
    // A bare "rom" name resolves against the app's external files dir.
    private fun romFromIntent(intent: Intent?): File? {
        if (intent == null) return null
        intent.getStringExtra("rom")?.let { arg ->
            val f = if (arg.startsWith("/")) File(arg) else File(storageDir(), arg)
            if (f.isFile) return f
        }
        intent.data?.path?.let { path ->
            val f = File(path)
            if (f.isFile) return f
        }
        return null
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        setIntent(intent)
        // launchRom() itself no-ops while a game is already running; a clean re-launch
        // wants `adb shell am force-stop com.grarak.dsvita` first.
        if (!inGame) romFromIntent(intent)?.let { launchRom(it) }
    }

    private fun matchParent() = FrameLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.MATCH_PARENT)

    override fun onWindowFocusChanged(hasFocus: Boolean) {
        super.onWindowFocusChanged(hasFocus)
        if (hasFocus && inGame) hideSystemBars()
    }

    private fun hideSystemBars() {
        WindowCompat.getInsetsController(window, window.decorView).apply {
            systemBarsBehavior = WindowInsetsControllerCompat.BEHAVIOR_SHOW_TRANSIENT_BARS_BY_SWIPE
            hide(WindowInsetsCompat.Type.systemBars())
        }
    }

    private fun showSystemBars() {
        WindowCompat.getInsetsController(window, window.decorView).show(WindowInsetsCompat.Type.systemBars())
    }

    // ------------------------------------------------------------------- rom browser

    private lateinit var romAdapter: RomAdapter

    private fun buildBrowser(): View {
        val column = LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            setBackgroundColor(getColor(R.color.bg))
        }
        val toolbar = MaterialToolbar(this).apply {
            title = "DSVita"
            setTitleTextColor(Color.WHITE)
            setBackgroundColor(getColor(R.color.surface))
        }
        column.addView(toolbar, LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT))

        val recycler = RecyclerView(this).apply {
            layoutManager = LinearLayoutManager(this@DSVitaActivity)
            setPadding(dp(8), dp(8), dp(8), dp(8))
            clipToPadding = false
        }
        romAdapter = RomAdapter(
            onPlay = { launchRom(it) },
            onSettings = { showGameSettings(it.absolutePath) },
        )
        recycler.adapter = romAdapter
        column.addView(recycler, LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, 0, 1f))
        return column
    }

    @SuppressLint("NotifyDataSetChanged")
    private fun refreshRomList() {
        roms.clear()
        storageDir().listFiles { _, name -> name.lowercase().endsWith(".nds") }?.let {
            roms.addAll(it.sorted())
        }
        romAdapter.submit(roms, storageDir().absolutePath)
    }

    private fun launchRom(rom: File) {
        if (inGame || !rom.isFile) return
        inGame = true
        paused = false
        browser.visibility = View.GONE
        controls.visibility = if (prefs().getBoolean("osc", true)) View.VISIBLE else View.GONE
        hideSystemBars()
        nativeLaunch(rom.absolutePath)
        uiHandler.removeCallbacks(debugTextPoll)
        uiHandler.post(debugTextPoll)
    }

    private fun prefs() = getSharedPreferences("dsvita", MODE_PRIVATE)

    // ------------------------------------------------------------------- pause dialog

    private fun showPauseDialog() {
        if (!paused) {
            paused = true
            nativePause()
        }
        val oscShown = prefs().getBoolean("osc", true)
        val items = arrayOf("Resume", "Settings", if (oscShown) "Hide on-screen controls" else "Show on-screen controls", "Quit to library")
        MaterialAlertDialogBuilder(this)
            .setTitle("Paused")
            .setItems(items) { _, which ->
                when (which) {
                    0 -> resumeGame()
                    1 -> showRuntimeSettings()
                    2 -> {
                        prefs().edit().putBoolean("osc", !oscShown).apply()
                        controls.visibility = if (!oscShown) View.VISIBLE else View.GONE
                        showPauseDialog()
                    }
                    3 -> quitToLibrary()
                }
            }
            .setOnCancelListener { resumeGame() }
            .show()
    }

    private fun resumeGame() {
        paused = false
        nativePauseChoice(PAUSE_RESUME)
    }

    private fun quitToLibrary() {
        inGame = false
        paused = false
        controls.visibility = View.GONE
        showSystemBars()
        refreshRomList()
        browser.visibility = View.VISIBLE
        nativePauseChoice(PAUSE_QUIT)
    }

    // ---------------------------------------------------------------------- settings

    private fun showGameSettings(romPath: String) {
        settingsScreen.show(
            title = File(romPath).name,
            json = JSONArray(nativeGetGameSettings(romPath)),
            runtime = false,
            onEdit = { idx, value -> nativeSetGameSetting(romPath, idx, value) },
            onClose = { settingsScreen.view.visibility = View.GONE },
        )
    }

    private fun showRuntimeSettings() {
        settingsScreen.show(
            title = "Settings",
            json = JSONArray(nativeGetRuntimeSettings()),
            runtime = true,
            onEdit = { idx, value -> nativeSetRuntimeSetting(idx, value) },
            onClose = {
                settingsScreen.view.visibility = View.GONE
                showPauseDialog()
            },
        )
    }

    // ------------------------------------------------------------------------- input

    private fun onSurfaceTouch(event: MotionEvent): Boolean {
        if (!inGame || paused) return false
        when (event.actionMasked) {
            MotionEvent.ACTION_DOWN, MotionEvent.ACTION_MOVE -> nativeTouch(event.x.toInt(), event.y.toInt(), true)
            MotionEvent.ACTION_UP, MotionEvent.ACTION_CANCEL -> nativeTouch(0, 0, false)
        }
        return true
    }

    // Physical gamepads plus a keyboard layer mirroring the Linux presenter
    // (WASD dpad, K/J/I/U = A/B/X/Y, B/V = Start/Select, 8/9 = L/R).
    private fun mapKey(keyCode: Int) = when (keyCode) {
        KeyEvent.KEYCODE_BUTTON_A -> DS_A
        KeyEvent.KEYCODE_BUTTON_B -> DS_B
        KeyEvent.KEYCODE_BUTTON_X -> DS_X
        KeyEvent.KEYCODE_BUTTON_Y -> DS_Y
        KeyEvent.KEYCODE_BUTTON_L1 -> DS_TRIGGER_L
        KeyEvent.KEYCODE_BUTTON_R1 -> DS_TRIGGER_R
        KeyEvent.KEYCODE_BUTTON_START -> DS_START
        KeyEvent.KEYCODE_BUTTON_SELECT -> DS_SELECT
        KeyEvent.KEYCODE_DPAD_UP -> DS_UP
        KeyEvent.KEYCODE_DPAD_DOWN -> DS_DOWN
        KeyEvent.KEYCODE_DPAD_LEFT -> DS_LEFT
        KeyEvent.KEYCODE_DPAD_RIGHT -> DS_RIGHT
        KeyEvent.KEYCODE_W -> DS_UP
        KeyEvent.KEYCODE_S -> DS_DOWN
        KeyEvent.KEYCODE_A -> DS_LEFT
        KeyEvent.KEYCODE_D -> DS_RIGHT
        KeyEvent.KEYCODE_K -> DS_A
        KeyEvent.KEYCODE_J -> DS_B
        KeyEvent.KEYCODE_I -> DS_X
        KeyEvent.KEYCODE_U -> DS_Y
        KeyEvent.KEYCODE_B -> DS_START
        KeyEvent.KEYCODE_V -> DS_SELECT
        KeyEvent.KEYCODE_8 -> DS_TRIGGER_L
        KeyEvent.KEYCODE_9 -> DS_TRIGGER_R
        else -> -1
    }

    override fun onKeyDown(keyCode: Int, event: KeyEvent): Boolean {
        if (keyCode == KeyEvent.KEYCODE_BACK) {
            when {
                settingsScreen.view.visibility == View.VISIBLE -> settingsScreen.back()
                inGame -> showPauseDialog()
                else -> return super.onKeyDown(keyCode, event)
            }
            return true
        }
        val dsKey = mapKey(keyCode)
        if (inGame && !paused && dsKey >= 0) {
            nativeKey(dsKey, true)
            return true
        }
        return super.onKeyDown(keyCode, event)
    }

    override fun onKeyUp(keyCode: Int, event: KeyEvent): Boolean {
        val dsKey = mapKey(keyCode)
        if (inGame && dsKey >= 0) {
            nativeKey(dsKey, false)
            return true
        }
        return super.onKeyUp(keyCode, event)
    }

    override fun surfaceCreated(holder: SurfaceHolder) = nativeSurfaceCreated(holder.surface)
    override fun surfaceChanged(holder: SurfaceHolder, format: Int, width: Int, height: Int) {}
    override fun surfaceDestroyed(holder: SurfaceHolder) = nativeSurfaceDestroyed()
}
