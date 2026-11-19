"""The key binding context for Enter on a blank line.

A key binding context cannot read the line above the cursor line. This
listener adds the context `piton_line_above_is_blank`. It is true when the
line above each cursor line is empty or holds only whitespace.
"""

import sublime
import sublime_plugin


class PitonEnterContext(sublime_plugin.EventListener):
    def on_query_context(self, view, key, operator, operand, match_all):
        if key != "piton_line_above_is_blank":
            return None

        def above_is_blank(region):
            row = view.rowcol(region.begin())[0]
            if row == 0:
                return False
            line = view.line(view.text_point(row - 1, 0))
            return view.substr(line).strip(" \t") == ""

        results = [above_is_blank(region) for region in view.sel()]
        value = all(results) if match_all else any(results)
        if operator == sublime.OP_EQUAL:
            return value == operand
        if operator == sublime.OP_NOT_EQUAL:
            return value != operand
        return None
