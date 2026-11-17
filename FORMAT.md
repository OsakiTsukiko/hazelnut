# The `.hzn` notebook format (version 1)

A Hazelnut notebook is a UTF-8 text file. It is line-oriented and readable
and editable by hand.

```text
%hazelnut 1
%title Field notes
%category #8fcf7a Outdoors

%chapter Monday
%tag Outdoors
Saw a {b|heron} by the river.
The water was {fg=#6aa8f0|very} {i,u|cold}.

%chapter Tuesday
{s|Rained all day.} Sun came out at 4.
```

## Structure

| Line                | Meaning                                                  |
|---------------------|----------------------------------------------------------|
| `%hazelnut <n>`     | Header. Must be the first non-blank line. `n` = version. |
| `%title <text>`     | Notebook title.                                          |
| `%category #rrggbb <name>` | Declares a category and its color (color optional). |
| `%chapter <title>`  | Starts a new chapter. Lines after it are its body.       |
| `%tag <name>`       | Inside a chapter: files it under the category `name`.    |
| `%% anything`       | Comment, ignored.                                        |
| `%<other> …`        | Unknown directive, ignored (for forward compatibility).  |
| anything else       | Body text of the current chapter.                        |

A chapter's body runs until the next `%chapter` or the end of the file.
Trailing blank lines in a body are not significant. Text that appears
before the first `%chapter` goes into an implicit chapter named "Untitled".

## Categories

Categories are declared with `%category`, normally right after `%title`, in
display order. A chapter belongs to at most one category, given by a `%tag`
line anywhere in its body (Hazelnut writes it right after `%chapter`). A
`%tag` naming an undeclared category creates it with a gray color.

Hazelnut shows chapters grouped by category: chapters without one first,
then each category's chapters in declaration order. Within a group, chapters
keep their order from the file. Older versions of Hazelnut ignore these
directives (and drop them when saving).

## Headings and lists

These are plain body text, written as in Markdown at the start of a line:

| Line start            | Meaning                         |
|-----------------------|---------------------------------|
| `# `, `## `, `### `   | heading, levels 1–3             |
| `- ` or `* `          | bullet list item                |
| `- [ ] ` / `- [x] `   | checklist item, open / done     |
| `> [!] `              | neutral callout / continuation |

List items can be nested with leading spaces or tabs (two spaces or one tab
per level). `-[ ]` without the space is accepted too. Hazelnut shows these
markers as headings, bullets and checkboxes, but stores them as typed.

Callouts require an explicit `> [!]` or `> [!color]` marker. A bare `>`
is ordinary text. Consecutive `> [!] ` lines continue the preceding callout. Its color is given on its first
line as `> [!color] …`, where `color` is one of `gray`, `red`, `orange`,
`yellow`, `green`, `teal`, `blue`, `purple`, `pink`, or `#rrggbb`. GitHub's
alert names (`NOTE`, `TIP`, `IMPORTANT`, `WARNING`, `CAUTION`) are read as
blue, green, purple, yellow and red. A line with its own `[!color]` starts a
new callout. Headings and lists can go inside a callout, after the `> [!…] ` prefix.

The literal typing toggle automatically backslash-escapes newly entered
formatting characters, e.g. `\# heading`, `\- item`, or `\> [!blue] text`.
These escapes have exactly the same meaning when entered manually in code
view. Ordinary letters and spaces keep their normal meaning. Existing text
is unaffected by the toggle.

## Fenced code blocks

A line of at least three backticks or tildes opens a code block. An optional
language follows the opening fence, for example ` ```rust `, ` ```python `,
` ```js `, ` ```json `, or `~~~sql`. A matching fence of the same character
and at least the opening length closes it. Up to three leading spaces are
allowed. Unclosed fences continue to the end of the chapter.

Code contents are raw: braces, backslashes, `%chapter`, headings, lists,
and callouts are preserved as typed. Inline style spans are not interpreted
inside code. Syntax highlighting is visual and is not stored as style spans.
Unknown or omitted languages use plain monospace text. Escape a fence's
backticks (`` \`\`\` ``) to display it as ordinary text outside a block.

## Inline styles

A styled span is written as `{spec|text}`, where `spec` is a comma-separated
list of:

| Token         | Effect                         |
|---------------|--------------------------------|
| `b`           | bold                           |
| `i`           | italic                         |
| `u`           | underline (solid)              |
| `u=<pattern>` | underline with a pattern: `solid`, `double`, `dotted`, `dashed`, `wavy`, `zigzag` |
| `s`           | strikethrough                  |
| `fg=#rrggbb`  | text color (`#rrggbbaa` too)   |
| `bg=#rrggbb`  | highlight / background color   |

Example: `{b,i,fg=#ef6f6c,bg=#22375a|important}`.

Spans don't nest and don't cross lines. Hazelnut writes one span per run of
identically styled characters. Unknown tokens in a spec are ignored.

## Escaping

In body text, `\` followed by any character produces that character
literally. Hazelnut escapes `\`, `{` and `}` when saving. A body line that
starts with `%` is written as `\%` so it isn't read as a directive.

When reading, a `{` that isn't followed by a valid `spec|` is treated as a
literal `{`.
