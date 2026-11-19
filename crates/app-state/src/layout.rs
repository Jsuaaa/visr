//! Tabs of panes: each tab is a binary tree of splits whose leaves show one terminal each.
//! Closing a pane only stops showing its terminal; the terminal lives on in the core.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// The smallest share of the space a split gives its first child.
const MIN_RATIO: f32 = 0.1;
/// The largest share of the space a split gives its first child.
const MAX_RATIO: f32 = 0.9;

/// The direction a split lays out its children.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Axis {
  /// Side by side.
  Horizontal,
  /// One above the other.
  Vertical,
}

/// One step down a split: which child to take.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Branch {
  /// The first child of the split.
  First,
  /// The second child of the split.
  Second,
}

/// A pane layout: a leaf shows one terminal, a split divides its space between two nodes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Node {
  /// A pane showing the terminal at this address.
  Leaf { terminal: String },
  /// Two nodes side by side or one above the other.
  Split {
    axis: Axis,
    /// The first child's share of the space.
    ratio: f32,
    first: Box<Node>,
    second: Box<Node>,
  },
}

impl Node {
  /// A pane that shows `terminal`.
  pub fn leaf(terminal: &str) -> Self {
    Self::Leaf {
      terminal: terminal.into(),
    }
  }

  /// An even split of `first` and `second` along `axis`.
  pub fn split(axis: Axis, first: Node, second: Node) -> Self {
    Self::Split {
      axis,
      ratio: 0.5,
      first: Box::new(first),
      second: Box::new(second),
    }
  }

  /// Terminals in reading order: left to right, top to bottom.
  pub fn leaves(&self) -> Vec<&str> {
    match self {
      Self::Leaf { terminal } => vec![terminal.as_str()],
      Self::Split { first, second, .. } => {
        let mut leaves = first.leaves();
        leaves.extend(second.leaves());
        leaves
      }
    }
  }

  /// The ratio of the split at `path`, if a split is there.
  pub fn ratio_at(&self, path: &[Branch]) -> Option<f32> {
    match (self, path.split_first()) {
      (Self::Split { ratio, .. }, None) => Some(*ratio),
      (Self::Split { first, .. }, Some((Branch::First, rest))) => first.ratio_at(rest),
      (Self::Split { second, .. }, Some((Branch::Second, rest))) => second.ratio_at(rest),
      (Self::Leaf { .. }, _) => None,
    }
  }

  /// The pixels the terminal's pane takes once nothing is moving, from the splits' ratios and
  /// the room `size` this node has; `divider` is what each split keeps for its handle.
  pub fn extent_of(&self, terminal: &str, size: (f32, f32), divider: f32) -> Option<(f32, f32)> {
    match self {
      Self::Leaf { terminal: shown } => (shown == terminal).then_some(size),
      Self::Split {
        axis,
        ratio,
        first,
        second,
      } => {
        let (width, height) = size;
        let (first_size, second_size) = match axis {
          Axis::Horizontal => {
            let first = width * ratio;
            (
              (first, height),
              ((width - first - divider).max(0.0), height),
            )
          }
          Axis::Vertical => {
            let first = height * ratio;
            ((width, first), (width, (height - first - divider).max(0.0)))
          }
        };
        first
          .extent_of(terminal, first_size, divider)
          .or_else(|| second.extent_of(terminal, second_size, divider))
      }
    }
  }

  /// The branches from here down to the terminal's leaf.
  pub fn path_to(&self, terminal: &str) -> Option<Vec<Branch>> {
    match self {
      Self::Leaf { terminal: shown } => (shown == terminal).then(Vec::new),
      Self::Split { first, second, .. } => {
        let (branch, path) = match first.path_to(terminal) {
          Some(path) => (Branch::First, path),
          None => (Branch::Second, second.path_to(terminal)?),
        };
        Some([vec![branch], path].concat())
      }
    }
  }

  fn contains(&self, terminal: &str) -> bool {
    self.leaves().contains(&terminal)
  }

  fn leaf_mut(&mut self, terminal: &str) -> Option<&mut Node> {
    match self {
      Self::Leaf { terminal: shown } if shown == terminal => Some(self),
      Self::Leaf { .. } => None,
      Self::Split { first, second, .. } => match first.leaf_mut(terminal) {
        Some(leaf) => Some(leaf),
        None => second.leaf_mut(terminal),
      },
    }
  }

  /// The tree without that terminal's leaf; `None` when nothing is left.
  fn without(self, terminal: &str) -> Option<Node> {
    match self {
      Self::Leaf { terminal: shown } if shown == terminal => None,
      leaf @ Self::Leaf { .. } => Some(leaf),
      Self::Split {
        axis,
        ratio,
        first,
        second,
      } => match (first.without(terminal), second.without(terminal)) {
        (Some(first), Some(second)) => Some(Self::Split {
          axis,
          ratio,
          first: Box::new(first),
          second: Box::new(second),
        }),
        (Some(only), None) | (None, Some(only)) => Some(only),
        (None, None) => None,
      },
    }
  }

  fn at_mut(&mut self, path: &[Branch]) -> Option<&mut Node> {
    let Some((step, rest)) = path.split_first() else {
      return Some(self);
    };
    match (self, step) {
      (Self::Split { first, .. }, Branch::First) => first.at_mut(rest),
      (Self::Split { second, .. }, Branch::Second) => second.at_mut(rest),
      (Self::Leaf { .. }, _) => None,
    }
  }
}

/// A size the new pane of a split will hold, from the size of the pane it splits: half, less what
/// the divider and the new header take, rounded down. A program never hears it is bigger than it
/// is; the exact size follows once the pane is laid out.
pub fn split_size((cols, rows): (u16, u16), axis: Axis) -> (u16, u16) {
  let half = |cells: u16, chrome: u16| (cells.saturating_sub(chrome) / 2).max(1);
  match axis {
    Axis::Horizontal => (half(cols, 4), rows),
    Axis::Vertical => (cols, half(rows, 6)),
  }
}

/// A tab: its tree of panes and which pane has focus.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Tab {
  pub root: Node,
  pub focused: String,
  /// The user's name for the tab, in place of its terminal's.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub name: Option<String>,
}

impl Tab {
  fn new(terminal: &str) -> Self {
    Self {
      root: Node::leaf(terminal),
      focused: terminal.into(),
      name: None,
    }
  }

  /// Drops the terminal's pane; `false` when the tab has nothing left.
  fn forget(&mut self, terminal: &str) -> bool {
    let index = self
      .root
      .leaves()
      .iter()
      .position(|shown| *shown == terminal);
    let root = std::mem::replace(&mut self.root, Node::leaf(""));
    let Some(root) = root.without(terminal) else {
      return false;
    };
    self.root = root;
    if self.focused == terminal {
      let leaves = self.root.leaves();
      let previous = index.unwrap_or(0).saturating_sub(1).min(leaves.len() - 1);
      self.focused = leaves[previous].to_string();
    }
    true
  }
}

/// Names a folder in the layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct FolderId(u32);

/// A named group of sidebar rows, kept in Visr's layout only.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Folder {
  pub id: FolderId,
  pub name: String,
  #[serde(default)]
  pub collapsed: bool,
  /// The terminals of its rows, in the order shown.
  #[serde(default)]
  pub terminals: Vec<String>,
}

/// One place in the sidebar's arranged order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Slot {
  /// A terminal outside any folder; layouts saved before folders hold only these.
  Terminal(String),
  Folder(Folder),
}

/// One line `order` lists, as `Layout::places` reads it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Place<'a> {
  Folder(&'a Folder),
  Terminal {
    address: &'a str,
    folder: Option<FolderId>,
  },
}

/// The tabs, which one is active, and the user's ordering and names for sidebar rows.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Layout {
  pub tabs: Vec<Tab>,
  pub active_tab: usize,
  /// The sidebar's order as the user arranged it; terminals missing from it go last.
  #[serde(default)]
  pub order: Vec<Slot>,
  /// The user's names for terminals shown in no tab, by address.
  #[serde(default)]
  pub names: BTreeMap<String, String>,
}

impl Layout {
  /// Every terminal some tab shows, tab by tab.
  pub fn shown(&self) -> Vec<&str> {
    self.tabs.iter().flat_map(|tab| tab.root.leaves()).collect()
  }

  /// The active tab, if any.
  pub fn active(&self) -> Option<&Tab> {
    self.tabs.get(self.active_tab)
  }

  fn active_mut(&mut self) -> Option<&mut Tab> {
    self.tabs.get_mut(self.active_tab)
  }

  /// The terminal in the focused pane of the active tab.
  pub fn focused(&self) -> Option<&str> {
    self.active().map(|tab| tab.focused.as_str())
  }

  /// Opens a tab showing the terminal and makes it active.
  pub fn new_tab(&mut self, terminal: &str) {
    self.tabs.push(Tab::new(terminal));
    self.active_tab = self.tabs.len() - 1;
  }

  /// Makes the tab at `index` active; out of range does nothing.
  pub fn select_tab(&mut self, index: usize) {
    if index < self.tabs.len() {
      self.active_tab = index;
    }
  }

  /// Focuses the terminal's pane if the active tab shows it.
  pub fn focus(&mut self, terminal: &str) {
    if let Some(tab) = self.active_mut()
      && tab.root.contains(terminal)
    {
      tab.focused = terminal.into();
    }
  }

  /// Moves focus to the next pane of the active tab, wrapping around.
  pub fn focus_next(&mut self) {
    self.step_focus(1);
  }

  /// Moves focus to the previous pane of the active tab, wrapping around.
  pub fn focus_prev(&mut self) {
    self.step_focus(-1);
  }

  fn step_focus(&mut self, step: isize) {
    let Some(tab) = self.active_mut() else { return };
    let leaves = tab.root.leaves();
    let Some(index) = leaves.iter().position(|shown| *shown == tab.focused) else {
      return;
    };
    let next = (index as isize + step).rem_euclid(leaves.len() as isize) as usize;
    tab.focused = leaves[next].to_string();
  }

  /// Shows the terminal next to the focused pane, in a new tab if there is none.
  pub fn split(&mut self, axis: Axis, terminal: &str) {
    let Some(tab) = self.active_mut() else {
      self.new_tab(terminal);
      return;
    };
    let focused = tab.focused.clone();
    if let Some(leaf) = tab.root.leaf_mut(&focused) {
      *leaf = Node::split(axis, Node::leaf(&focused), Node::leaf(terminal));
      tab.focused = terminal.into();
    }
  }

  /// Stops showing the focused pane; a tab left empty closes.
  pub fn close_focused(&mut self) {
    let Some(focused) = self.focused().map(str::to_string) else {
      return;
    };
    let active = self.active_tab;
    if !self.tabs[active].forget(&focused) {
      self.tabs.remove(active);
      self.active_tab = active.min(self.tabs.len().saturating_sub(1));
    }
  }

  /// Closes the active tab and all its panes; their terminals live on.
  pub fn close_tab(&mut self) {
    if self.active_tab < self.tabs.len() {
      self.tabs.remove(self.active_tab);
      self.active_tab = self.active_tab.min(self.tabs.len().saturating_sub(1));
    }
  }

  /// Every place `order` lists, depth first: a folder, then its terminals. A terminal listed
  /// again later keeps its first place.
  pub(crate) fn places(&self) -> Vec<Place<'_>> {
    let mut places = Vec::new();
    for slot in &self.order {
      match slot {
        Slot::Terminal(address) => places.push(Place::Terminal {
          address,
          folder: None,
        }),
        Slot::Folder(folder) => {
          places.push(Place::Folder(folder));
          places.extend(folder.terminals.iter().map(|address| Place::Terminal {
            address,
            folder: Some(folder.id),
          }));
        }
      }
    }
    let mut seen: Vec<&str> = Vec::new();
    places.retain(|place| match place {
      Place::Terminal { address, .. } => {
        let first = !seen.contains(address);
        seen.push(address);
        first
      }
      Place::Folder(_) => true,
    });
    places
  }

  /// Moves the sidebar row at `from` to `to`, given the rows as shown.
  pub fn move_entry(&mut self, entries: &[crate::Entry], from: usize, to: usize) {
    let mut entries = entries.to_vec();
    if from >= entries.len() || to >= entries.len() {
      return;
    }
    let moved = entries.remove(from);
    entries.insert(to, moved);
    self.order = entries
      .iter()
      .flat_map(|entry| entry.terminals())
      .map(|address| Slot::Terminal(address.to_string()))
      .collect();
  }

  /// Names a sidebar row for this view only; the terminals keep theirs. Blank clears it.
  pub fn rename(&mut self, entry: &crate::Entry, name: &str) {
    let name = Some(name.trim().to_string()).filter(|name| !name.is_empty());
    match (entry, name) {
      (crate::Entry::Tab { index, .. }, name) => {
        if let Some(tab) = self.tabs.get_mut(*index) {
          tab.name = name;
        }
      }
      (crate::Entry::Loose(address), Some(name)) => {
        self.names.insert(address.clone(), name);
      }
      (crate::Entry::Loose(address), None) => {
        self.names.remove(address);
      }
    }
  }

  /// The terminal is gone from the core: drop its panes everywhere.
  pub fn forget(&mut self, terminal: &str) {
    self
      .order
      .retain(|slot| !matches!(slot, Slot::Terminal(known) if known == terminal));
    for slot in &mut self.order {
      if let Slot::Folder(folder) = slot {
        folder.terminals.retain(|known| known != terminal);
      }
    }
    self.names.remove(terminal);
    self.tabs.retain_mut(|tab| tab.forget(terminal));
    self.active_tab = self.active_tab.min(self.tabs.len().saturating_sub(1));
  }

  /// Forgets every shown terminal not in `terminals`: its panes, its place in the sidebar order
  /// and its name.
  pub fn keep_only(&mut self, terminals: &[&str]) {
    let shown: Vec<String> = self.shown().into_iter().map(str::to_string).collect();
    for terminal in shown
      .iter()
      .filter(|shown| !terminals.contains(&shown.as_str()))
    {
      self.forget(terminal);
    }
  }

  /// Brings a terminal into view: its tab if it has one, else in place of the focused pane.
  pub fn show(&mut self, terminal: &str) {
    if let Some(index) = self.tabs.iter().position(|tab| tab.root.contains(terminal)) {
      self.active_tab = index;
      self.tabs[index].focused = terminal.into();
      return;
    }
    let Some(tab) = self.active_mut() else {
      self.new_tab(terminal);
      return;
    };
    let focused = tab.focused.clone();
    if let Some(leaf) = tab.root.leaf_mut(&focused) {
      *leaf = Node::leaf(terminal);
      tab.focused = terminal.into();
    }
  }

  /// Sets the split at `path` in the active tab, kept between `MIN_RATIO` and `MAX_RATIO`.
  pub fn set_ratio(&mut self, path: &[Branch], ratio: f32) {
    if let Some(tab) = self.active_mut()
      && let Some(Node::Split { ratio: current, .. }) = tab.root.at_mut(path)
    {
      *current = ratio.clamp(MIN_RATIO, MAX_RATIO);
    }
  }
}

/// Builders the tests of this crate share.
#[cfg(test)]
pub(crate) mod testing {
  use super::*;

  pub(crate) fn id(number: u32) -> FolderId {
    FolderId(number)
  }

  pub(crate) fn top(address: &str) -> Slot {
    Slot::Terminal(address.into())
  }

  /// A folder named `f{number}`.
  pub(crate) fn folder(number: u32, collapsed: bool, terminals: &[&str]) -> Slot {
    Slot::Folder(Folder {
      id: id(number),
      name: format!("f{number}"),
      collapsed,
      terminals: terminals
        .iter()
        .map(|address| address.to_string())
        .collect(),
    })
  }

  /// One single-pane tab per address, so the last one is active.
  pub(crate) fn layout_with(tabs: &[&str], order: Vec<Slot>) -> Layout {
    let mut layout = Layout::default();
    for address in tabs {
      layout.new_tab(address);
    }
    layout.order = order;
    layout
  }
}

#[cfg(test)]
mod tests {
  use super::testing::{folder, id, layout_with, top};
  use super::*;

  #[test]
  fn reads_a_layout_saved_before_folders() {
    let json = r#"{"tabs":[{"root":{"leaf":{"terminal":"t1"}},"focused":"t1"}],
    "active_tab":0,"order":["t1","t2"],"names":{"t2":"logs"}}"#;
    let layout: Layout = serde_json::from_str(json).expect("a 0.1.1 layout reads");
    assert_eq!(layout.order, vec![top("t1"), top("t2")]);
    assert_eq!(layout.shown(), vec!["t1"]);
    assert_eq!(layout.names.get("t2").map(String::as_str), Some("logs"));
  }

  #[test]
  fn folders_are_saved_in_this_shape_and_read_back() {
    let layout = layout_with(&[], vec![top("t1"), folder(1, true, &["t2"])]);
    let saved = serde_json::to_value(&layout).expect("a layout serializes");
    assert_eq!(
      saved["order"],
      serde_json::json!(["t1", {"id": 1, "name": "f1", "collapsed": true, "terminals": ["t2"]}])
    );
    let read: Layout = serde_json::from_value(saved).expect("it reads back");
    assert_eq!(read, layout);
  }

  #[test]
  fn a_folder_without_its_optional_fields_reads() {
    let json = r#"{"tabs":[],"active_tab":0,"order":[{"id":2,"name":"api"}]}"#;
    let layout: Layout = serde_json::from_str(json).expect("a short folder reads");
    let api = Folder {
      id: id(2),
      name: "api".into(),
      collapsed: false,
      terminals: Vec::new(),
    };
    assert_eq!(layout.order, vec![Slot::Folder(api)]);
  }

  #[test]
  fn places_read_depth_first_and_skip_a_terminal_listed_again() {
    let layout = layout_with(
      &[],
      vec![top("t1"), folder(1, false, &["t2", "t1"]), top("t3")],
    );
    let listed = Folder {
      id: id(1),
      name: "f1".into(),
      collapsed: false,
      terminals: vec!["t2".into(), "t1".into()],
    };
    assert_eq!(
      layout.places(),
      vec![
        Place::Terminal {
          address: "t1",
          folder: None
        },
        Place::Folder(&listed),
        Place::Terminal {
          address: "t2",
          folder: Some(id(1))
        },
        Place::Terminal {
          address: "t3",
          folder: None
        },
      ]
    );
  }
}
