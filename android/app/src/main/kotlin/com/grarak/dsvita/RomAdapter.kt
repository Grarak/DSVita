package com.grarak.dsvita

import android.annotation.SuppressLint
import android.graphics.Color
import android.view.Gravity
import android.view.View
import android.view.ViewGroup
import android.widget.ImageView
import android.widget.LinearLayout
import android.widget.TextView
import androidx.recyclerview.widget.RecyclerView
import com.google.android.material.card.MaterialCardView
import java.io.File

// Material rom cards: title + subtitle, whole card plays, a settings glyph opens the
// per-game settings.
class RomAdapter(
    private val onPlay: (File) -> Unit,
    private val onSettings: (File) -> Unit,
) : RecyclerView.Adapter<RomAdapter.Holder>() {

    private var roms: List<File> = emptyList()
    private var emptyHint: String = ""

    @SuppressLint("NotifyDataSetChanged")
    fun submit(list: List<File>, dir: String) {
        roms = list
        emptyHint = "No games found\nPush .nds files to $dir"
        notifyDataSetChanged()
    }

    override fun getItemCount() = if (roms.isEmpty()) 1 else roms.size

    class Holder(val card: MaterialCardView, val title: TextView, val subtitle: TextView, val settings: ImageView) :
        RecyclerView.ViewHolder(card)

    override fun onCreateViewHolder(parent: ViewGroup, viewType: Int): Holder {
        val ctx = parent.context
        val d = ctx.resources.displayMetrics.density
        fun px(v: Int) = (v * d).toInt()

        val card = MaterialCardView(ctx).apply {
            layoutParams = RecyclerView.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT).apply {
                setMargins(px(8), px(6), px(8), px(6))
            }
            radius = px(16).toFloat()
            cardElevation = 0f
            setCardBackgroundColor(ctx.getColor(R.color.surface_container))
            isClickable = true
            isFocusable = true
        }
        val row = LinearLayout(ctx).apply {
            orientation = LinearLayout.HORIZONTAL
            gravity = Gravity.CENTER_VERTICAL
            setPadding(px(20), px(18), px(12), px(18))
        }
        val texts = LinearLayout(ctx).apply {
            orientation = LinearLayout.VERTICAL
            layoutParams = LinearLayout.LayoutParams(0, ViewGroup.LayoutParams.WRAP_CONTENT, 1f)
        }
        val title = TextView(ctx).apply {
            textSize = 17f
            setTextColor(Color.WHITE)
        }
        val subtitle = TextView(ctx).apply {
            textSize = 12f
            setTextColor(Color.parseColor("#FF8A9398"))
        }
        texts.addView(title)
        texts.addView(subtitle)
        row.addView(texts)

        val settings = ImageView(ctx).apply {
            setImageResource(android.R.drawable.ic_menu_manage)
            setColorFilter(ctx.getColor(R.color.teal_200))
            val s = px(24)
            layoutParams = LinearLayout.LayoutParams(px(48), px(48))
            setPadding((px(48) - s) / 2, (px(48) - s) / 2, (px(48) - s) / 2, (px(48) - s) / 2)
        }
        row.addView(settings)
        card.addView(row)
        return Holder(card, title, subtitle, settings)
    }

    override fun onBindViewHolder(holder: Holder, position: Int) {
        if (roms.isEmpty()) {
            holder.title.text = "No games found"
            holder.subtitle.text = emptyHint.substringAfter('\n')
            holder.settings.visibility = View.GONE
            holder.card.setOnClickListener(null)
            holder.card.isClickable = false
            return
        }
        val rom = roms[position]
        holder.settings.visibility = View.VISIBLE
        holder.card.isClickable = true
        holder.title.text = rom.nameWithoutExtension
        holder.subtitle.text = "Nintendo DS"
        holder.card.setOnClickListener { onPlay(rom) }
        holder.settings.setOnClickListener { onSettings(rom) }
    }
}
