//! The sidebar as the person arranged it: rows at the top level and in folders.

use crate::layout::{FolderId, Layout, Place};
use crate::{Entry, Terminals};

/// A folder as the sidebar shows it, with its rows in order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SidebarFolder {
  pub id: FolderId,
  pub name: String,
  pub collapsed: bool,
  pub rows: Vec<Entry>,
}

/// One line at the sidebar's top level: a row, or a folder with its rows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SidebarItem {
  Row(Entry),
  Folder(SidebarFolder),
}

/// The sidebar as shown: every tab, then every loose terminal that isn't leaving, each placed by
/// its best ranked terminal in `layout.order`. Terminals the order doesn't list go last, in the
/// core's order, at the top level.
pub fn sidebar_items(layout: &Layout, terminals: &Terminals) -> Vec<SidebarItem> {
  let tabs = layout
    .tabs
    .iter()
    .enumerate()
    .map(|(index, tab)| Entry::Tab {
      index,
      terminals: tab.root.leaves().into_iter().map(str::to_string).collect(),
    });
  let shown = layout.shown();
  let loose = terminals
    .list()
    .iter()
    // One about to be removed would flash in the list until it is.
    .filter(|terminal| {
      !shown.contains(&terminal.address.as_str()) && !terminals.is_leaving(terminal)
    })
    .map(|terminal| Entry::Loose(terminal.address.clone()));
  let places = layout.places();
  let arranged = places.len();
  let rank = |address: &str| {
    let listed = places
      .iter()
      .enumerate()
      .find_map(|(index, place)| match place {
        Place::Terminal {
          address: known,
          folder,
        } if *known == address => Some((index, *folder)),
        _ => None,
      });
    listed.unwrap_or_else(|| {
      let core = terminals
        .list()
        .iter()
        .position(|terminal| terminal.address == address);
      (arranged + core.unwrap_or(usize::MAX - arranged), None)
    })
  };
  let mut rows: Vec<(usize, Option<FolderId>, Entry)> = tabs
    .chain(loose)
    .filter_map(|entry| {
      let (rank, folder) = entry
        .terminals()
        .into_iter()
        .map(rank)
        .min_by_key(|(rank, _)| *rank)?;
      Some((rank, folder, entry))
    })
    .collect();
  rows.sort_by_key(|(rank, ..)| *rank);

  let mut items: Vec<(usize, SidebarItem)> = Vec::new();
  for (index, place) in places.iter().enumerate() {
    if let Place::Folder(folder) = place {
      let rows = rows
        .iter()
        .filter(|(_, held, _)| *held == Some(folder.id))
        .map(|(.., entry)| entry.clone())
        .collect();
      items.push((
        index,
        SidebarItem::Folder(SidebarFolder {
          id: folder.id,
          name: folder.name.clone(),
          collapsed: folder.collapsed,
          rows,
        }),
      ));
    }
  }
  items.extend(
    rows
      .into_iter()
      .filter(|(_, folder, _)| folder.is_none())
      .map(|(rank, _, entry)| (rank, SidebarItem::Row(entry))),
  );
  items.sort_by_key(|(rank, _)| *rank);
  items.into_iter().map(|(_, item)| item).collect()
}

/// The row holding this terminal, at the top level or in a folder.
pub fn find_row<'a>(items: &'a [SidebarItem], terminal: &str) -> Option<&'a Entry> {
  let holds = |entry: &&Entry| entry.terminals().contains(&terminal);
  items.iter().find_map(|item| match item {
    SidebarItem::Row(entry) => Some(entry).filter(holds),
    SidebarItem::Folder(folder) => folder.rows.iter().find(holds),
  })
}

/// The folder holding this terminal's row; `None` at the top level or with no such row.
pub fn folder_of(items: &[SidebarItem], terminal: &str) -> Option<FolderId> {
  items.iter().find_map(|item| match item {
    SidebarItem::Folder(folder)
      if folder
        .rows
        .iter()
        .any(|entry| entry.terminals().contains(&terminal)) =>
    {
      Some(folder.id)
    }
    _ => None,
  })
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::layout::testing::{core, folder, id, layout_with, top};
  use crate::layout::{Axis, Node};

  fn loose(address: &str) -> Entry {
    Entry::Loose(address.into())
  }

  fn row(address: &str) -> SidebarItem {
    SidebarItem::Row(loose(address))
  }

  fn in_folder(number: u32, collapsed: bool, rows: &[&str]) -> SidebarItem {
    SidebarItem::Folder(SidebarFolder {
      id: id(number),
      name: format!("f{number}"),
      collapsed,
      rows: rows.iter().map(|address| loose(address)).collect(),
    })
  }

  #[test]
  fn folders_keep_their_place_and_unlisted_rows_go_last() {
    let core = core(&["t1", "t2", "t3", "t4", "t5"]);
    let layout = layout_with(
      &[],
      vec![top("t4"), folder(1, false, &["t3", "t1"]), top("t2")],
    );
    assert_eq!(
      sidebar_items(&layout, &core),
      vec![
        row("t4"),
        in_folder(1, false, &["t3", "t1"]),
        row("t2"),
        row("t5")
      ]
    );
  }

  #[test]
  fn a_tab_sits_in_the_folder_of_its_best_ranked_terminal() {
    let core = core(&["t1", "t2", "t3"]);
    let mut layout = layout_with(
      &["t3"],
      vec![folder(1, false, &["t2"]), top("t3"), top("t1")],
    );
    layout.tabs[0].root = Node::split(Axis::Horizontal, Node::leaf("t3"), Node::leaf("t2"));
    let tab = Entry::Tab {
      index: 0,
      terminals: vec!["t3".into(), "t2".into()],
    };
    let f1 = SidebarFolder {
      id: id(1),
      name: "f1".into(),
      collapsed: false,
      rows: vec![tab],
    };
    assert_eq!(
      sidebar_items(&layout, &core),
      vec![SidebarItem::Folder(f1), row("t1")]
    );
  }

  #[test]
  fn collapsed_and_empty_folders_still_show_with_their_rows() {
    let core = core(&["t1"]);
    let layout = layout_with(&[], vec![folder(1, true, &["t1"]), folder(2, false, &[])]);
    assert_eq!(
      sidebar_items(&layout, &core),
      vec![in_folder(1, true, &["t1"]), in_folder(2, false, &[])]
    );
  }

  #[test]
  fn an_address_listed_twice_keeps_its_first_place() {
    let core = core(&["t1", "t2"]);
    let layout = layout_with(&[], vec![top("t1"), folder(1, false, &["t1", "t2"])]);
    assert_eq!(
      sidebar_items(&layout, &core),
      vec![row("t1"), in_folder(1, false, &["t2"])]
    );
  }

  #[test]
  fn a_folder_of_gone_terminals_shows_empty() {
    let core = core(&["t1"]);
    let layout = layout_with(&[], vec![folder(1, false, &["t9"]), top("t1")]);
    assert_eq!(
      sidebar_items(&layout, &core),
      vec![in_folder(1, false, &[]), row("t1")]
    );
  }

  #[test]
  fn arranging_the_sidebar_as_shown_keeps_its_order() {
    let core = core(&["t1", "t2", "t3"]);
    let mut layout = layout_with(&[], vec![top("t2"), folder(1, true, &["t3"]), top("t1")]);
    let before = layout.order.clone();
    let items = sidebar_items(&layout, &core);
    layout.arrange(&items);
    assert_eq!(layout.order, before);
  }

  #[test]
  fn arranging_lists_unlisted_rows_and_drops_gone_terminals() {
    let core = core(&["t1", "t2"]);
    let mut layout = layout_with(&[], vec![folder(1, false, &["t9", "t2"])]);
    let items = sidebar_items(&layout, &core);
    layout.arrange(&items);
    assert_eq!(layout.order, vec![folder(1, false, &["t2"]), top("t1")]);
  }

  #[test]
  fn rows_are_found_inside_folders_too() {
    let items = vec![row("t1"), in_folder(1, false, &["t2"])];
    assert_eq!(find_row(&items, "t2"), Some(&loose("t2")));
    assert_eq!(find_row(&items, "t9"), None);
    assert_eq!(folder_of(&items, "t2"), Some(id(1)));
    assert_eq!(folder_of(&items, "t1"), None);
  }
}
