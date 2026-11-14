# The `.hzn` notebook format (version 1)

A Hazelnut notebook is a UTF-8 text file. It is line-oriented and readable
and editable by hand.

```text
%hazelnut 1
%title Field notes

%chapter Monday
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
| `%chapter <title>`  | Starts a new chapter. Lines after it are its body.       |
| `%% anything`       | Comment, ignored.                                        |
| `%<other> …`        | Unknown directive, ignored (for forward compatibility).  |
| anything else       | Body text of the current chapter.                        |

A chapter's body runs until the next `%chapter` or the end of the file.
Trailing blank lines in a body are not significant. Text that appears
before the first `%chapter` goes into an implicit chapter named "Untitled".

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
