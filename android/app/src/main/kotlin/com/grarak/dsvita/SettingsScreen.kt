package com.grarak.dsvita

import android.app.Activity
import android.graphics.Color
import android.view.Gravity
import android.view.View
import android.view.ViewGroup
import android.widget.LinearLayout
import android.widget.TextView
import androidx.recyclerview.widget.LinearLayoutManager
import androidx.recyclerview.widget.RecyclerView
import com.google.android.material.appbar.MaterialToolbar
import com.google.android.material.dialog.MaterialAlertDialogBuilder
import com.google.android.material.materialswitch.MaterialSwitch
import org.json.JSONArray
import org.json.JSONObject

// A full-screen Material settings surface rendered generically from the JSON the native
// side serializes out of the Rust setting definitions. Bools become switches, lists open
// a single-choice dialog; entries are grouped under category headers. In runtime mode
// only runtime-changeable settings are enabled.
class SettingsScreen(private val activity: Activity) {

    private sealed class Item
    private class Header(val text: String) : Item()
    private class Entry(val setting: JSONObject) : Item()

    val view: LinearLayout
    private val recycler: RecyclerView
    private val toolbar: MaterialToolbar
    private var items: List<Item> = emptyList()
    private var runtime = false
    private var onEdit: (Int, Int) -> Unit = { _, _ -> }
    private var onClose: () -> Unit = {}

    private fun dp(v: Int) = (v * activity.resources.displayMetrics.density).toInt()

    init {
        view = LinearLayout(activity).apply {
            orientation = LinearLayout.VERTICAL
            setBackgroundColor(activity.getColor(R.color.bg))
        }
        toolbar = MaterialToolbar(activity).apply {
            setBackgroundColor(activity.getColor(R.color.surface))
            setTitleTextColor(Color.WHITE)
            navigationIcon = activity.getDrawable(androidx.appcompat.R.drawable.abc_ic_ab_back_material)
            setNavigationOnClickListener { back() }
        }
        view.addView(toolbar, LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT))
        recycler = RecyclerView(activity).apply {
            layoutManager = LinearLayoutManager(activity)
            adapter = Adapter()
        }
        view.addView(recycler, LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, 0, 1f))
    }

    fun show(title: String, json: JSONArray, runtime: Boolean, onEdit: (Int, Int) -> Unit, onClose: () -> Unit) {
        this.runtime = runtime
        this.onEdit = onEdit
        this.onClose = onClose
        toolbar.title = title
        items = layout(json)
        recycler.adapter?.notifyDataSetChanged()
        view.visibility = View.VISIBLE
    }

    fun back() = onClose()

    private fun layout(json: JSONArray): List<Item> {
        val out = ArrayList<Item>()
        for (group in listOf("Emulation", "Graphics", "Screen", "System")) {
            var headerAdded = false
            for (i in 0 until json.length()) {
                val s = json.getJSONObject(i)
                if (s.getString("group") != group || s.getString("kind") == "int") continue
                if (!headerAdded) {
                    out.add(Header(group)); headerAdded = true
                }
                out.add(Entry(s))
            }
        }
        return out
    }

    private inner class Adapter : RecyclerView.Adapter<RecyclerView.ViewHolder>() {
        override fun getItemViewType(position: Int) = if (items[position] is Header) 0 else 1
        override fun getItemCount() = items.size

        override fun onCreateViewHolder(parent: ViewGroup, viewType: Int): RecyclerView.ViewHolder {
            return if (viewType == 0) {
                val tv = TextView(activity).apply {
                    textSize = 14f
                    setTextColor(activity.getColor(R.color.teal_200))
                    setPadding(dp(20), dp(20), dp(20), dp(6))
                    layoutParams = RecyclerView.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT)
                }
                object : RecyclerView.ViewHolder(tv) {}
            } else {
                EntryHolder(activity)
            }
        }

        override fun onBindViewHolder(holder: RecyclerView.ViewHolder, position: Int) {
            val item = items[position]
            if (item is Header) {
                (holder.itemView as TextView).text = item.text
            } else {
                (holder as EntryHolder).bind((item as Entry).setting)
            }
        }
    }

    private inner class EntryHolder(activity: Activity) : RecyclerView.ViewHolder(
        LinearLayout(activity).apply {
            orientation = LinearLayout.HORIZONTAL
            gravity = Gravity.CENTER_VERTICAL
            setPadding(dp(20), dp(16), dp(20), dp(16))
            layoutParams = RecyclerView.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT)
            isClickable = true
        }
    ) {
        private val row = itemView as LinearLayout
        private val title = TextView(activity).apply { textSize = 16f }
        private val desc = TextView(activity).apply {
            textSize = 12f
            setTextColor(Color.parseColor("#FF8A9398"))
        }
        private val value = TextView(activity).apply {
            textSize = 15f
            setTextColor(activity.getColor(R.color.teal_200))
        }
        private val toggle = MaterialSwitch(activity)

        init {
            val texts = LinearLayout(activity).apply {
                orientation = LinearLayout.VERTICAL
                layoutParams = LinearLayout.LayoutParams(0, ViewGroup.LayoutParams.WRAP_CONTENT, 1f)
            }
            texts.addView(title)
            texts.addView(desc)
            row.addView(texts)
            row.addView(value)
            row.addView(toggle)
        }

        fun bind(s: JSONObject) {
            val enabled = !runtime || s.getBoolean("runtime")
            val idx = s.getInt("idx")
            title.text = s.getString("title")
            title.setTextColor(if (enabled) Color.WHITE else Color.parseColor("#FF5A6266"))
            desc.text = s.getString("desc")

            if (s.getString("kind") == "bool") {
                value.visibility = View.GONE
                toggle.visibility = View.VISIBLE
                toggle.setOnCheckedChangeListener(null)
                toggle.isChecked = s.getBoolean("value")
                toggle.isEnabled = enabled
                toggle.setOnCheckedChangeListener { _, checked ->
                    s.put("value", checked)
                    onEdit(idx, if (checked) 1 else 0)
                }
                row.setOnClickListener(if (enabled) View.OnClickListener { toggle.toggle() } else null)
            } else {
                toggle.visibility = View.GONE
                value.visibility = View.VISIBLE
                val options = s.getJSONArray("options")
                value.text = if (options.length() > 0) options.getString(s.getInt("selection")) else ""
                value.setTextColor(activity.getColor(if (enabled) R.color.teal_200 else R.color.surface_high))
                row.setOnClickListener(
                    if (enabled) View.OnClickListener { pickList(s) } else null
                )
            }
            row.isClickable = enabled
        }

        private fun pickList(s: JSONObject) {
            val options = s.getJSONArray("options")
            val values = Array(options.length()) { options.getString(it) }
            MaterialAlertDialogBuilder(activity)
                .setTitle(s.getString("title"))
                .setSingleChoiceItems(values, s.getInt("selection")) { dialog, which ->
                    s.put("selection", which)
                    onEdit(s.getInt("idx"), which)
                    value.text = values[which]
                    dialog.dismiss()
                }
                .show()
        }
    }
}
