# Visual answer / 视觉回答 / 視覚回答

This sign asks visitors to **keep the passage clear**, *check before taking photos*, and ~~ignore the old opening hours~~. **Bold with *nested italic* and ~~strikethrough~~** should remain readable on the same line.

中文：请保持通道畅通。日本語：通路をふさがないでください。English: keep the exit clear. Combining characters: é, ä. Emoji: 👨‍👩‍👧‍👦 and 👍🏽.

## Recommended steps

3. Read the sign before entering. This long list item needs a hanging indent when its text wraps onto another line, including at the largest supported font size.
   - Check the exit and leave space for other visitors.
   - Read the [visitor guide](https://example.com/guide).
4. Follow staff instructions.

- [x] Locate the exit.
- [ ] Confirm the current opening hours.

> The quoted instruction is **important**.
> It stays indented and keeps its left border across pages.
>
> > A nested quote must also remain readable.

---

### Code and literal text

Use `photo_allowed && !blocking_exit` as an inline example. Code must display `**asterisks**` and `<tags>` literally.

```rust
fn can_take_photo(allowed: bool, passage_clear: bool) -> bool {
    // Keep indentation and this empty line.

    allowed && passage_clear
}
let long_line = "This deliberately long line continues across several visual lines without an ellipsis or missing characters, even at the largest font size supported by the wrist panel.";
```

    Indented code keeps **literal** Markdown.
      Its spaces remain visible.

#### Table data

| Item | Meaning | Action |
| --- | --- | --- |
| Exit / 出口 | **Keep clear** | Do not block the doorway or narrow the passage. |
| Photos / 拍照 | Ask first | Check with staff before taking any photos. |
| Hours / 時間 | *May change* | Read the current notice. |

##### Links and images

The URL <https://example.com/very/long/path/with/a/query?visitor=first&language=en> is visible but cannot be clicked. This image is shown only as its label: ![Entrance map / 入口示意图](https://example.com/map.png).

<script>do_not_execute_or_display_this()</script>

Visible text around <b>inline HTML tags</b> stays visible.

###### End of section

A hard break follows here.  
This sentence starts on the next line. Text without a closing marker remains visible: **unfinished emphasis.
