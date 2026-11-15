//! A single line of editable text with a selection, the model behind a text field.
//! Offsets are bytes into the text, always on char boundaries; IME ranges are UTF-16.

use std::ops::Range;

/// A line of text with a selection and an optional IME composition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct EditBuffer {
  text: String,
  selection: Range<usize>,
  /// The cursor is at the selection's start rather than its end.
  reversed: bool,
  /// Text an IME is still composing, like a dead key's accent.
  marked: Option<Range<usize>>,
}

impl EditBuffer {
  /// Starts with the whole text selected, as a rename does.
  pub(crate) fn new(text: &str) -> Self {
    Self {
      text: text.to_string(),
      selection: 0..text.len(),
      reversed: false,
      marked: None,
    }
  }

  pub(crate) fn text(&self) -> &str {
    &self.text
  }

  pub(crate) fn selection(&self) -> Range<usize> {
    self.selection.clone()
  }

  pub(crate) fn cursor(&self) -> usize {
    if self.reversed {
      self.selection.start
    } else {
      self.selection.end
    }
  }

  pub(crate) fn selected_text(&self) -> &str {
    &self.text[self.selection.clone()]
  }

  pub(crate) fn marked(&self) -> Option<Range<usize>> {
    self.marked.clone()
  }

  pub(crate) fn select_all(&mut self) {
    self.selection = 0..self.text.len();
    self.reversed = false;
  }

  pub(crate) fn place(&mut self, offset: usize) {
    self.selection = offset..offset;
    self.reversed = false;
  }

  /// Moves the cursor to `offset`, keeping the other end of the selection where it is.
  pub(crate) fn select_to(&mut self, offset: usize) {
    let anchor = if self.reversed {
      self.selection.end
    } else {
      self.selection.start
    };
    self.reversed = offset < anchor;
    self.selection = anchor.min(offset)..anchor.max(offset);
  }

  fn go(&mut self, offset: usize, extend: bool) {
    if extend {
      self.select_to(offset);
    } else {
      self.place(offset);
    }
  }

  /// Replaces what is composing, else the selection. A field is one line, so pasted lines join.
  pub(crate) fn insert(&mut self, text: &str) {
    let text = if text.contains('\n') {
      text.lines().collect::<Vec<_>>().join(" ")
    } else {
      text.to_string()
    };
    let range = self.marked.take().unwrap_or(self.selection.clone());
    self.text.replace_range(range.clone(), &text);
    self.place(range.start + text.len());
  }

  pub(crate) fn backspace(&mut self) {
    if self.selection.is_empty() {
      self.select_to(self.previous(self.cursor()));
    }
    self.insert("");
  }

  pub(crate) fn delete(&mut self) {
    if self.selection.is_empty() {
      self.select_to(self.next(self.cursor()));
    }
    self.insert("");
  }

  pub(crate) fn delete_word_back(&mut self) {
    if self.selection.is_empty() {
      self.select_to(self.word_start(self.cursor()));
    }
    self.insert("");
  }

  pub(crate) fn delete_to_start(&mut self) {
    if self.selection.is_empty() {
      self.select_to(0);
    }
    self.insert("");
  }

  pub(crate) fn move_left(&mut self, extend: bool) {
    if !extend && !self.selection.is_empty() {
      return self.place(self.selection.start);
    }
    self.go(self.previous(self.cursor()), extend);
  }

  pub(crate) fn move_right(&mut self, extend: bool) {
    if !extend && !self.selection.is_empty() {
      return self.place(self.selection.end);
    }
    self.go(self.next(self.cursor()), extend);
  }

  pub(crate) fn move_word_left(&mut self, extend: bool) {
    self.go(self.word_start(self.cursor()), extend);
  }

  pub(crate) fn move_word_right(&mut self, extend: bool) {
    self.go(self.word_end(self.cursor()), extend);
  }

  pub(crate) fn move_home(&mut self, extend: bool) {
    self.go(0, extend);
  }

  pub(crate) fn move_end(&mut self, extend: bool) {
    self.go(self.text.len(), extend);
  }

  fn previous(&self, offset: usize) -> usize {
    self.text[..offset]
      .char_indices()
      .next_back()
      .map_or(0, |(index, _)| index)
  }

  fn next(&self, offset: usize) -> usize {
    self.text[offset..]
      .chars()
      .next()
      .map_or(offset, |c| offset + c.len_utf8())
  }

  /// Back over spaces, then over the word before them.
  fn word_start(&self, offset: usize) -> usize {
    let before = self.text[..offset].trim_end();
    before
      .char_indices()
      .rev()
      .find(|(_, c)| c.is_whitespace())
      .map_or(0, |(index, c)| index + c.len_utf8())
  }

  /// Over spaces, then to the end of the word after them.
  fn word_end(&self, offset: usize) -> usize {
    let after = &self.text[offset..];
    let start = after.len() - after.trim_start().len();
    after[start..]
      .find(char::is_whitespace)
      .map_or(self.text.len(), |end| offset + start + end)
  }

  pub(crate) fn offset_to_utf16(&self, offset: usize) -> usize {
    self.text[..offset].encode_utf16().count()
  }

  pub(crate) fn offset_from_utf16(&self, utf16: usize) -> usize {
    let mut units = 0;
    for (index, c) in self.text.char_indices() {
      if units >= utf16 {
        return index;
      }
      units += c.len_utf16();
    }
    self.text.len()
  }

  fn range_from_utf16(&self, range: &Range<usize>) -> Range<usize> {
    self.offset_from_utf16(range.start)..self.offset_from_utf16(range.end)
  }

  fn range_to_utf16(&self, range: &Range<usize>) -> Range<usize> {
    self.offset_to_utf16(range.start)..self.offset_to_utf16(range.end)
  }

  /// The text in a UTF-16 range, clamped to the text, and the range actually covered.
  pub(crate) fn text_utf16(&self, range: Range<usize>) -> (String, Range<usize>) {
    let end = self.offset_to_utf16(self.text.len());
    let covered = range.start.min(end)..range.end.min(end);
    let bytes = self.range_from_utf16(&covered);
    (self.text[bytes].to_string(), covered)
  }

  pub(crate) fn selected_range_utf16(&self) -> Range<usize> {
    self.range_to_utf16(&self.selection)
  }

  pub(crate) fn marked_range_utf16(&self) -> Option<Range<usize>> {
    self.marked.as_ref().map(|range| self.range_to_utf16(range))
  }

  /// Commits text from the IME over `range`, else over what is composing or selected.
  pub(crate) fn replace_utf16(&mut self, range: Option<Range<usize>>, text: &str) {
    if let Some(range) = range {
      self.marked = None;
      self.selection = self.range_from_utf16(&range);
    }
    self.insert(text);
  }

  /// Shows text the IME is still composing, marked, with `selected` inside it.
  pub(crate) fn mark_utf16(
    &mut self,
    range: Option<Range<usize>>,
    text: &str,
    selected: Option<Range<usize>>,
  ) {
    let range = match range {
      Some(range) => self.range_from_utf16(&range),
      None => self.marked.clone().unwrap_or(self.selection.clone()),
    };
    self.text.replace_range(range.clone(), text);
    self.marked = (!text.is_empty()).then(|| range.start..range.start + text.len());
    let end = range.start + text.len();
    // `selected` counts UTF-16 units from the start of the marked text.
    let base = self.offset_to_utf16(range.start);
    self.selection = match selected {
      Some(selected) => self.range_from_utf16(&(base + selected.start..base + selected.end)),
      None => end..end,
    };
    self.reversed = false;
  }

  pub(crate) fn unmark(&mut self) {
    self.marked = None;
  }
}
