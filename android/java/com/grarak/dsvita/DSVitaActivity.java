package com.grarak.dsvita;

import android.app.Activity;
import android.app.AlertDialog;
import android.graphics.Color;
import android.os.Bundle;
import android.view.KeyEvent;
import android.view.MotionEvent;
import android.view.Surface;
import android.view.SurfaceHolder;
import android.view.SurfaceView;
import android.view.View;
import android.view.WindowManager;
import android.widget.ArrayAdapter;
import android.widget.FrameLayout;
import android.widget.ListView;

import java.io.File;
import java.util.ArrayList;
import java.util.Collections;

// All UI lives here (rom list, pause dialog); the native side only renders emulator
// frames into the SurfaceView and blocks in present_ui/present_pause until this
// Activity calls nativeLaunch/nativePauseChoice. See presenter/android.rs for the
// full JNI contract.
public class DSVitaActivity extends Activity implements SurfaceHolder.Callback {
    static {
        System.loadLibrary("dsvita");
    }

    private static native void nativeInit(String storagePath);

    private static native void nativeSurfaceCreated(Surface surface);

    private static native void nativeSurfaceDestroyed();

    private static native void nativeLaunch(String romPath);

    private static native void nativeTouch(int x, int y, boolean down);

    private static native void nativeKey(int dsKey, boolean down);

    private static native void nativePause();

    private static native void nativePauseChoice(int choice);

    private static native void nativeResume();

    // input::Keycode discriminants (src/core/input.rs)
    private static final int DS_A = 0, DS_B = 1, DS_SELECT = 2, DS_START = 3, DS_RIGHT = 4, DS_LEFT = 5, DS_UP = 6, DS_DOWN = 7, DS_TRIGGER_R = 8, DS_TRIGGER_L = 9, DS_X = 10, DS_Y = 11;

    private static final int PAUSE_RESUME = 0, PAUSE_QUIT = 2;

    private SurfaceView surfaceView;
    private ListView romList;
    private final ArrayList<File> roms = new ArrayList<>();
    private boolean inGame = false;
    private static boolean initialized = false;

    @Override
    protected void onCreate(Bundle savedInstanceState) {
        super.onCreate(savedInstanceState);
        getWindow().addFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON);

        FrameLayout root = new FrameLayout(this);
        root.setBackgroundColor(Color.BLACK);

        surfaceView = new SurfaceView(this);
        surfaceView.getHolder().addCallback(this);
        surfaceView.setOnTouchListener(this::onSurfaceTouch);
        root.addView(surfaceView, new FrameLayout.LayoutParams(FrameLayout.LayoutParams.MATCH_PARENT, FrameLayout.LayoutParams.MATCH_PARENT));

        romList = new ListView(this);
        romList.setBackgroundColor(Color.argb(230, 16, 16, 16));
        romList.setOnItemClickListener((parent, view, position, id) -> launchRom(roms.get(position)));
        root.addView(romList, new FrameLayout.LayoutParams(FrameLayout.LayoutParams.MATCH_PARENT, FrameLayout.LayoutParams.MATCH_PARENT));

        setContentView(root);

        if (!initialized) {
            initialized = true;
            nativeInit(getExternalFilesDir(null).getAbsolutePath());
        }
        refreshRomList();
    }

    @Override
    public void onWindowFocusChanged(boolean hasFocus) {
        super.onWindowFocusChanged(hasFocus);
        if (hasFocus) {
            getWindow().getDecorView().setSystemUiVisibility(View.SYSTEM_UI_FLAG_IMMERSIVE_STICKY | View.SYSTEM_UI_FLAG_FULLSCREEN | View.SYSTEM_UI_FLAG_HIDE_NAVIGATION | View.SYSTEM_UI_FLAG_LAYOUT_STABLE);
        }
    }

    private void refreshRomList() {
        roms.clear();
        File dir = getExternalFilesDir(null);
        File[] files = dir == null ? null : dir.listFiles((d, name) -> name.toLowerCase().endsWith(".nds"));
        ArrayList<String> names = new ArrayList<>();
        if (files != null) {
            for (File file : files) {
                roms.add(file);
            }
            Collections.sort(roms);
            for (File file : roms) {
                names.add(file.getName());
            }
        }
        if (names.isEmpty()) {
            names.add("No roms found — push .nds files to " + (dir == null ? "external storage" : dir.getAbsolutePath()));
        }
        romList.setAdapter(new ArrayAdapter<String>(this, android.R.layout.simple_list_item_1, names) {
            @Override
            public View getView(int position, View convertView, android.view.ViewGroup parent) {
                android.widget.TextView view = (android.widget.TextView) super.getView(position, convertView, parent);
                view.setTextSize(24);
                view.setPadding(48, 40, 48, 40);
                return view;
            }
        });
    }

    private void launchRom(File rom) {
        if (inGame || !rom.isFile()) {
            return;
        }
        inGame = true;
        romList.setVisibility(View.GONE);
        nativeLaunch(rom.getAbsolutePath());
    }

    private void showPauseDialog() {
        nativePause();
        new AlertDialog.Builder(this)
                .setTitle("Paused")
                .setItems(new String[]{"Resume", "Quit"}, (dialog, which) -> {
                    if (which == 1) {
                        inGame = false;
                        refreshRomList();
                        romList.setVisibility(View.VISIBLE);
                        nativePauseChoice(PAUSE_QUIT);
                    } else {
                        nativePauseChoice(PAUSE_RESUME);
                    }
                })
                .setOnCancelListener(dialog -> nativePauseChoice(PAUSE_RESUME))
                .show();
    }

    private boolean onSurfaceTouch(View view, MotionEvent event) {
        if (!inGame) {
            return false;
        }
        switch (event.getActionMasked()) {
            case MotionEvent.ACTION_DOWN:
            case MotionEvent.ACTION_MOVE:
                nativeTouch((int) event.getX(), (int) event.getY(), true);
                return true;
            case MotionEvent.ACTION_UP:
            case MotionEvent.ACTION_CANCEL:
                nativeTouch(0, 0, false);
                return true;
        }
        return false;
    }

    // Physical gamepads plus a keyboard layer mirroring the Linux presenter
    // (WASD dpad, K/J/I/U = A/B/X/Y, B/V = Start/Select, 8/9 = L/R).
    private static int mapKey(int keyCode) {
        switch (keyCode) {
            case KeyEvent.KEYCODE_BUTTON_A: return DS_A;
            case KeyEvent.KEYCODE_BUTTON_B: return DS_B;
            case KeyEvent.KEYCODE_BUTTON_X: return DS_X;
            case KeyEvent.KEYCODE_BUTTON_Y: return DS_Y;
            case KeyEvent.KEYCODE_BUTTON_L1: return DS_TRIGGER_L;
            case KeyEvent.KEYCODE_BUTTON_R1: return DS_TRIGGER_R;
            case KeyEvent.KEYCODE_BUTTON_START: return DS_START;
            case KeyEvent.KEYCODE_BUTTON_SELECT: return DS_SELECT;
            case KeyEvent.KEYCODE_DPAD_UP: return DS_UP;
            case KeyEvent.KEYCODE_DPAD_DOWN: return DS_DOWN;
            case KeyEvent.KEYCODE_DPAD_LEFT: return DS_LEFT;
            case KeyEvent.KEYCODE_DPAD_RIGHT: return DS_RIGHT;
            case KeyEvent.KEYCODE_W: return DS_UP;
            case KeyEvent.KEYCODE_S: return DS_DOWN;
            case KeyEvent.KEYCODE_A: return DS_LEFT;
            case KeyEvent.KEYCODE_D: return DS_RIGHT;
            case KeyEvent.KEYCODE_K: return DS_A;
            case KeyEvent.KEYCODE_J: return DS_B;
            case KeyEvent.KEYCODE_I: return DS_X;
            case KeyEvent.KEYCODE_U: return DS_Y;
            case KeyEvent.KEYCODE_B: return DS_START;
            case KeyEvent.KEYCODE_V: return DS_SELECT;
            case KeyEvent.KEYCODE_8: return DS_TRIGGER_L;
            case KeyEvent.KEYCODE_9: return DS_TRIGGER_R;
            default: return -1;
        }
    }

    @Override
    public boolean onKeyDown(int keyCode, KeyEvent event) {
        if (keyCode == KeyEvent.KEYCODE_BACK) {
            if (inGame) {
                showPauseDialog();
                return true;
            }
            return super.onKeyDown(keyCode, event);
        }
        int dsKey = mapKey(keyCode);
        if (inGame && dsKey >= 0) {
            nativeKey(dsKey, true);
            return true;
        }
        return super.onKeyDown(keyCode, event);
    }

    @Override
    public boolean onKeyUp(int keyCode, KeyEvent event) {
        int dsKey = mapKey(keyCode);
        if (inGame && dsKey >= 0) {
            nativeKey(dsKey, false);
            return true;
        }
        return super.onKeyUp(keyCode, event);
    }

    @Override
    public void surfaceCreated(SurfaceHolder holder) {
        nativeSurfaceCreated(holder.getSurface());
    }

    @Override
    public void surfaceChanged(SurfaceHolder holder, int format, int width, int height) {
    }

    @Override
    public void surfaceDestroyed(SurfaceHolder holder) {
        nativeSurfaceDestroyed();
    }
}
