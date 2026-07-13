package com.grarak.dsvita

import android.app.Activity
import android.app.AlertDialog
import android.graphics.Canvas
import android.graphics.Color
import android.graphics.Paint
import android.graphics.RectF
import android.os.Bundle
import android.view.Gravity
import android.view.KeyEvent
import android.view.MotionEvent
import android.view.Surface
import android.view.SurfaceHolder
import android.view.SurfaceView
import android.view.View
import android.view.ViewGroup
import android.view.WindowManager
import android.widget.BaseAdapter
import android.widget.FrameLayout
import android.widget.LinearLayout
import android.widget.ListView
import android.widget.Switch
import android.widget.TextView
import org.json.JSONArray
import org.json.JSONObject
import java.io.File

// All UI lives here (rom browser, settings, pause dialog, on-screen controls); the
// native side renders emulator frames into the SurfaceView and blocks in
// present_ui/present_pause until this Activity calls nativeLaunch/nativePauseChoice.
// Settings render generically from JSON serialized out of the Rust definitions — the
// native side stays the single source of truth. See presenter/android.rs.
class DSVitaActivity : Activity(), SurfaceHolder.Callback {
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

    private lateinit var root: FrameLayout
    private lateinit var surfaceView: SurfaceView
    private lateinit var romList: ListView
    private lateinit var settingsPanel: LinearLayout
    private lateinit var controls: OnScreenControls
    private val roms = ArrayList<File>()
    private var inGame = false
    private var paused = false

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        window.addFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)

        root = FrameLayout(this)

        surfaceView = SurfaceView(this)
        surfaceView.holder.addCallback(this)
        surfaceView.setOnTouchListener { _, event -> onSurfaceTouch(event) }
        root.addView(surfaceView, matchParent())

        controls = OnScreenControls(this) { key, down -> if (inGame) nativeKey(key, down) }
        controls.visibility = View.GONE
        root.addView(controls, matchParent())

        romList = ListView(this)
        romList.setBackgroundColor(Color.argb(235, 18, 18, 20))
        romList.setOnItemClickListener { _, _, position, _ -> launchRom(roms[position]) }
        romList.setOnItemLongClickListener { _, _, position, _ ->
            showSettings(roms[position].absolutePath, runtime = false)
            true
        }
        root.addView(romList, matchParent())

        settingsPanel = LinearLayout(this)
        settingsPanel.orientation = LinearLayout.VERTICAL
        settingsPanel.setBackgroundColor(Color.argb(245, 18, 18, 20))
        settingsPanel.visibility = View.GONE
        root.addView(settingsPanel, matchParent())

        setContentView(root)

        if (!initialized) {
            initialized = true
            nativeInit(getExternalFilesDir(null)!!.absolutePath)
        }
        refreshRomList()
    }

    private fun matchParent() = FrameLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.MATCH_PARENT)

    override fun onWindowFocusChanged(hasFocus: Boolean) {
        super.onWindowFocusChanged(hasFocus)
        if (hasFocus) {
            @Suppress("DEPRECATION")
            window.decorView.systemUiVisibility = View.SYSTEM_UI_FLAG_IMMERSIVE_STICKY or
                View.SYSTEM_UI_FLAG_FULLSCREEN or View.SYSTEM_UI_FLAG_HIDE_NAVIGATION or View.SYSTEM_UI_FLAG_LAYOUT_STABLE
        }
    }

    // ------------------------------------------------------------------- rom browser

    private fun refreshRomList() {
        roms.clear()
        val dir = getExternalFilesDir(null)
        dir?.listFiles { _, name -> name.lowercase().endsWith(".nds") }?.let {
            roms.addAll(it.sorted())
        }
        val names = if (roms.isEmpty()) {
            listOf("No roms found — push .nds files to ${dir?.absolutePath}")
        } else {
            roms.map { it.name }
        }
        romList.adapter = object : BaseAdapter() {
            override fun getCount() = names.size
            override fun getItem(position: Int) = names[position]
            override fun getItemId(position: Int) = position.toLong()
            override fun getView(position: Int, convertView: View?, parent: ViewGroup?): View {
                val row = (convertView as? LinearLayout) ?: LinearLayout(this@DSVitaActivity).apply {
                    orientation = LinearLayout.VERTICAL
                    setPadding(48, 34, 48, 34)
                    addView(TextView(this@DSVitaActivity).apply { textSize = 22f; setTextColor(Color.WHITE) })
                    addView(TextView(this@DSVitaActivity).apply { textSize = 12f; setTextColor(Color.GRAY) })
                }
                (row.getChildAt(0) as TextView).text = names[position]
                (row.getChildAt(1) as TextView).text = if (roms.isEmpty()) "" else "tap to play — hold for settings"
                return row
            }
        }
    }

    private fun launchRom(rom: File) {
        if (inGame || !rom.isFile) return
        inGame = true
        paused = false
        romList.visibility = View.GONE
        controls.visibility = if (prefs().getBoolean("osc", true)) View.VISIBLE else View.GONE
        nativeLaunch(rom.absolutePath)
    }

    private fun prefs() = getSharedPreferences("dsvita", MODE_PRIVATE)

    // ------------------------------------------------------------------- pause dialog

    private fun showPauseDialog() {
        if (!paused) {
            paused = true
            nativePause()
        }
        val oscShown = prefs().getBoolean("osc", true)
        val items = arrayOf("Resume", "Settings", if (oscShown) "Hide on-screen controls" else "Show on-screen controls", "Quit")
        AlertDialog.Builder(this)
            .setTitle("Paused")
            .setItems(items) { _, which ->
                when (which) {
                    0 -> resumeGame()
                    1 -> showSettings(null, runtime = true)
                    2 -> {
                        prefs().edit().putBoolean("osc", !oscShown).apply()
                        controls.visibility = if (!oscShown) View.VISIBLE else View.GONE
                        showPauseDialog()
                    }
                    3 -> {
                        inGame = false
                        paused = false
                        controls.visibility = View.GONE
                        refreshRomList()
                        romList.visibility = View.VISIBLE
                        nativePauseChoice(PAUSE_QUIT)
                    }
                }
            }
            .setOnCancelListener { resumeGame() }
            .show()
    }

    private fun resumeGame() {
        paused = false
        nativePauseChoice(PAUSE_RESUME)
    }

    // ---------------------------------------------------------------------- settings

    // Generic renderer over the JSON the native side serializes from the Rust setting
    // definitions: bools become switches, lists open a single-choice dialog. In runtime
    // mode only runtime-changeable settings are enabled and edits queue natively until
    // the pause resolves.
    private fun showSettings(romPath: String?, runtime: Boolean) {
        val json = JSONArray(if (runtime) nativeGetRuntimeSettings() else nativeGetGameSettings(romPath!!))
        settingsPanel.removeAllViews()

        val title = TextView(this).apply {
            text = if (runtime) "Settings" else "Settings — ${File(romPath!!).name}"
            textSize = 24f
            setTextColor(Color.WHITE)
            setPadding(48, 40, 48, 24)
        }
        settingsPanel.addView(title)

        data class Row(val header: String?, val setting: JSONObject?)

        val rows = ArrayList<Row>()
        for (group in listOf("Emulation", "Graphics", "Screen", "System")) {
            var headerAdded = false
            for (i in 0 until json.length()) {
                val s = json.getJSONObject(i)
                if (s.getString("group") != group || s.getString("kind") == "int") continue
                if (!headerAdded) {
                    rows.add(Row(group, null)); headerAdded = true
                }
                rows.add(Row(null, s))
            }
        }

        val list = ListView(this)
        val adapter = object : BaseAdapter() {
            override fun getCount() = rows.size
            override fun getItem(position: Int) = rows[position]
            override fun getItemId(position: Int) = position.toLong()
            override fun isEnabled(position: Int): Boolean {
                val s = rows[position].setting ?: return false
                return !runtime || s.getBoolean("runtime")
            }

            override fun getView(position: Int, convertView: View?, parent: ViewGroup?): View {
                val row = rows[position]
                if (row.header != null) {
                    return TextView(this@DSVitaActivity).apply {
                        text = row.header
                        textSize = 15f
                        setTextColor(Color.rgb(130, 170, 255))
                        setPadding(48, 34, 48, 10)
                    }
                }
                val s = row.setting!!
                val enabled = !runtime || s.getBoolean("runtime")
                val line = LinearLayout(this@DSVitaActivity).apply {
                    orientation = LinearLayout.HORIZONTAL
                    gravity = Gravity.CENTER_VERTICAL
                    setPadding(48, 22, 48, 22)
                }
                val texts = LinearLayout(this@DSVitaActivity).apply {
                    orientation = LinearLayout.VERTICAL
                    layoutParams = LinearLayout.LayoutParams(0, ViewGroup.LayoutParams.WRAP_CONTENT, 1f)
                }
                texts.addView(TextView(this@DSVitaActivity).apply {
                    text = s.getString("title")
                    textSize = 18f
                    setTextColor(if (enabled) Color.WHITE else Color.GRAY)
                })
                texts.addView(TextView(this@DSVitaActivity).apply {
                    text = s.getString("desc")
                    textSize = 12f
                    setTextColor(Color.GRAY)
                })
                line.addView(texts)
                if (s.getString("kind") == "bool") {
                    line.addView(Switch(this@DSVitaActivity).apply {
                        isChecked = s.getBoolean("value")
                        isEnabled = enabled
                        setOnCheckedChangeListener { _, checked ->
                            s.put("value", checked)
                            applySetting(romPath, runtime, s.getInt("idx"), if (checked) 1 else 0)
                        }
                    })
                } else {
                    line.addView(TextView(this@DSVitaActivity).apply {
                        val options = s.getJSONArray("options")
                        text = if (options.length() > 0) options.getString(s.getInt("selection")) else ""
                        textSize = 16f
                        setTextColor(if (enabled) Color.rgb(130, 170, 255) else Color.DKGRAY)
                    })
                }
                return line
            }
        }
        list.adapter = adapter
        list.setOnItemClickListener { _, _, position, _ ->
            val s = rows[position].setting ?: return@setOnItemClickListener
            if (s.getString("kind") != "list") return@setOnItemClickListener
            val options = s.getJSONArray("options")
            val values = Array(options.length()) { options.getString(it) }
            AlertDialog.Builder(this)
                .setTitle(s.getString("title"))
                .setSingleChoiceItems(values, s.getInt("selection")) { dialog, which ->
                    s.put("selection", which)
                    applySetting(romPath, runtime, s.getInt("idx"), which)
                    dialog.dismiss()
                    adapter.notifyDataSetChanged()
                }
                .show()
        }
        settingsPanel.addView(list, LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, 0, 1f))
        settingsPanel.visibility = View.VISIBLE
    }

    private fun applySetting(romPath: String?, runtime: Boolean, idx: Int, value: Int) {
        if (runtime) nativeSetRuntimeSetting(idx, value) else nativeSetGameSetting(romPath!!, idx, value)
    }

    private fun closeSettings() {
        settingsPanel.visibility = View.GONE
        if (inGame && paused) showPauseDialog()
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
                settingsPanel.visibility == View.VISIBLE -> closeSettings()
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

// Translucent touch controls drawn over the game: dpad (8-way zones), A/B/X/Y, L/R,
// Start/Select. Tracks every pointer per frame and diffs the held set into
// nativeKey presses, so multi-touch chords and slides across the dpad work.
class OnScreenControls(context: Activity, private val onKey: (Int, Boolean) -> Unit) : View(context) {
    private data class Button(val key: Int, val label: String, var rect: RectF = RectF())

    private val buttons = listOf(
        Button(DSVitaActivity.DS_A, "A"),
        Button(DSVitaActivity.DS_B, "B"),
        Button(DSVitaActivity.DS_X, "X"),
        Button(DSVitaActivity.DS_Y, "Y"),
        Button(DSVitaActivity.DS_TRIGGER_L, "L"),
        Button(DSVitaActivity.DS_TRIGGER_R, "R"),
        Button(DSVitaActivity.DS_START, "START"),
        Button(DSVitaActivity.DS_SELECT, "SELECT"),
    )
    private var dpadCenterX = 0f
    private var dpadCenterY = 0f
    private var dpadRadius = 0f
    private val held = HashSet<Int>()
    private val fill = Paint(Paint.ANTI_ALIAS_FLAG).apply { color = Color.argb(60, 255, 255, 255) }
    private val stroke = Paint(Paint.ANTI_ALIAS_FLAG).apply {
        color = Color.argb(140, 255, 255, 255)
        style = Paint.Style.STROKE
        strokeWidth = 3f
    }
    private val text = Paint(Paint.ANTI_ALIAS_FLAG).apply {
        color = Color.argb(190, 255, 255, 255)
        textAlign = Paint.Align.CENTER
    }

    override fun onSizeChanged(w: Int, h: Int, oldw: Int, oldh: Int) {
        val u = minOf(w, h) / 100f
        dpadCenterX = 16f * u
        dpadCenterY = h - 24f * u
        dpadRadius = 13f * u

        val faceX = w - 16f * u
        val faceY = h - 24f * u
        val r = 5.5f * u
        fun circle(cx: Float, cy: Float) = RectF(cx - r, cy - r, cx + r, cy + r)
        buttons[0].rect = circle(faceX + 8f * u, faceY) // A
        buttons[1].rect = circle(faceX, faceY + 8f * u) // B
        buttons[2].rect = circle(faceX, faceY - 8f * u) // X
        buttons[3].rect = circle(faceX - 8f * u, faceY) // Y
        buttons[4].rect = RectF(2f * u, 2f * u, 20f * u, 9f * u) // L
        buttons[5].rect = RectF(w - 20f * u, 2f * u, w - 2f * u, 9f * u) // R
        buttons[6].rect = RectF(w / 2f + 4f * u, h - 8f * u, w / 2f + 20f * u, h - 2f * u) // START
        buttons[7].rect = RectF(w / 2f - 20f * u, h - 8f * u, w / 2f - 4f * u, h - 2f * u) // SELECT
        text.textSize = 4f * u
    }

    override fun onDraw(canvas: Canvas) {
        // dpad
        canvas.drawCircle(dpadCenterX, dpadCenterY, dpadRadius, fill)
        canvas.drawCircle(dpadCenterX, dpadCenterY, dpadRadius, stroke)
        val a = dpadRadius * 0.55f
        text.textSize = dpadRadius * 0.32f
        canvas.drawText("▲", dpadCenterX, dpadCenterY - a + text.textSize / 2, text)
        canvas.drawText("▼", dpadCenterX, dpadCenterY + a + text.textSize / 2, text)
        canvas.drawText("◀", dpadCenterX - a, dpadCenterY + text.textSize / 2, text)
        canvas.drawText("▶", dpadCenterX + a, dpadCenterY + text.textSize / 2, text)
        for (button in buttons) {
            val r = button.rect
            if (button.label.length == 1) {
                canvas.drawCircle(r.centerX(), r.centerY(), r.width() / 2, fill)
                canvas.drawCircle(r.centerX(), r.centerY(), r.width() / 2, stroke)
                text.textSize = r.width() * 0.5f
            } else {
                canvas.drawRoundRect(r, r.height() / 2, r.height() / 2, fill)
                canvas.drawRoundRect(r, r.height() / 2, r.height() / 2, stroke)
                text.textSize = r.height() * 0.42f
            }
            canvas.drawText(button.label, r.centerX(), r.centerY() + text.textSize * 0.35f, text)
        }
    }

    private var tracking = false

    private fun collect(x: Float, y: Float, out: MutableSet<Int>) {
        var hit = false
        for (button in buttons) {
            if (button.rect.contains(x, y)) {
                out.add(button.key)
                hit = true
            }
        }
        if (!hit) {
            val dx = x - dpadCenterX
            val dy = y - dpadCenterY
            val dist = Math.hypot(dx.toDouble(), dy.toDouble()).toFloat()
            if (dist < dpadRadius * 1.35f && dist > dpadRadius * 0.15f) {
                if (dx > Math.abs(dy) * 0.45f) out.add(DSVitaActivity.DS_RIGHT)
                if (-dx > Math.abs(dy) * 0.45f) out.add(DSVitaActivity.DS_LEFT)
                if (dy > Math.abs(dx) * 0.45f) out.add(DSVitaActivity.DS_DOWN)
                if (-dy > Math.abs(dx) * 0.45f) out.add(DSVitaActivity.DS_UP)
            }
        }
    }

    override fun onTouchEvent(event: MotionEvent): Boolean {
        // Consume a stream only if it starts on a control; everything else falls
        // through to the game surface (DS touchscreen).
        if (event.actionMasked == MotionEvent.ACTION_DOWN) {
            val probe = HashSet<Int>()
            collect(event.getX(0), event.getY(0), probe)
            tracking = probe.isNotEmpty()
            if (!tracking) return false
        }
        if (!tracking) return false

        val now = HashSet<Int>()
        val ending = event.actionMasked == MotionEvent.ACTION_UP || event.actionMasked == MotionEvent.ACTION_CANCEL
        if (!ending) {
            for (p in 0 until event.pointerCount) {
                if (event.actionMasked == MotionEvent.ACTION_POINTER_UP && p == event.actionIndex) continue
                collect(event.getX(p), event.getY(p), now)
            }
        }
        for (key in now) if (key !in held) onKey(key, true)
        for (key in held) if (key !in now) onKey(key, false)
        held.clear()
        held.addAll(now)
        if (ending) tracking = false
        return true
    }
}
