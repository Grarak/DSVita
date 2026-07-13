package com.grarak.dsvita

import android.content.Context
import android.graphics.Canvas
import android.graphics.Color
import android.graphics.Paint
import android.graphics.RectF
import android.view.MotionEvent
import android.view.View

// Translucent multi-touch gamepad drawn over the game: 8-way dpad, ABXY diamond, L/R
// shoulders, Start/Select. Tracks every pointer per frame and diffs the held set into
// nativeKey presses, so chords and dpad slides work. A stream that doesn't start on a
// control is ignored so it falls through to the DS touchscreen underneath.
class OnScreenControls(context: Context, private val onKey: (Int, Boolean) -> Unit) : View(context) {
    private class Button(val key: Int, val label: String, var rect: RectF = RectF())

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
    private var tracking = false

    private val fill = Paint(Paint.ANTI_ALIAS_FLAG).apply { color = Color.argb(46, 255, 255, 255) }
    private val stroke = Paint(Paint.ANTI_ALIAS_FLAG).apply {
        color = Color.argb(120, 255, 255, 255)
        style = Paint.Style.STROKE
        strokeWidth = 3f
    }
    private val text = Paint(Paint.ANTI_ALIAS_FLAG).apply {
        color = Color.argb(200, 255, 255, 255)
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
        canvas.drawCircle(dpadCenterX, dpadCenterY, dpadRadius, fill)
        canvas.drawCircle(dpadCenterX, dpadCenterY, dpadRadius, stroke)
        val a = dpadRadius * 0.55f
        text.textSize = dpadRadius * 0.32f
        val ty = text.textSize / 2
        canvas.drawText("▲", dpadCenterX, dpadCenterY - a + ty, text)
        canvas.drawText("▼", dpadCenterX, dpadCenterY + a + ty, text)
        canvas.drawText("◀", dpadCenterX - a, dpadCenterY + ty, text)
        canvas.drawText("▶", dpadCenterX + a, dpadCenterY + ty, text)
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
