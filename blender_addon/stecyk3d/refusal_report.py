# SPDX-License-Identifier: GPL-3.0-or-later
# (The refusal report). Pure Python: the wording is tested without Blender.
"""What a refused render says: every problem on its own line, and a short summary for the status bar."""

REPORT_TEXT = "Stecyk3D report"  # the name of the text datablock the full list is written to
SEE_REPORT = "see Text Editor > " + REPORT_TEXT
SUMMARY_WIDTH = 90  # the status bar has room for about this much of the first problem


def lines_of(*texts):
    """The non-empty lines of each text (or each item of a list), in order, without repeats of a line already given."""
    seen, out = set(), []
    for text in texts:
        for part in (text if isinstance(text, (list, tuple)) else [text]):
            for line in str(part).splitlines():
                line = line.strip()
                if line and line not in seen:
                    seen.add(line)
                    out.append(line)
    return out


def summary(head, lines):
    """One short line: the head, how many problems, the first one cut to the status bar's width, where the rest is."""
    first = lines[0] if lines else ""
    if len(first) > SUMMARY_WIDTH:
        first = first[:SUMMARY_WIDTH - 3].rstrip() + "..."
    count = "%d problems" % len(lines) if len(lines) != 1 else "1 problem"
    return "%s: %s (%s); %s" % (head, first, count, SEE_REPORT) if lines else "%s; %s" % (head, SEE_REPORT)


def text_of(head, lines):
    """The text datablock's content: the head, then one problem per line."""
    return "\n".join([head + ":"] + ["  " + line for line in lines]) + "\n"
