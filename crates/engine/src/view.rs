//! Library view state: what the grid shows and what is selected.

use lightcraft_catalog::{AlbumId, Catalog, Filter, PhotoId};
use serde::{Deserialize, Serialize};

/// The "My Photos" source in the left panel.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum LibrarySource {
    #[default]
    All,
    /// Imported in the last import session (we treat "recent" as the latest import date).
    RecentlyAdded,
    Album(AlbumId),
    RecentlyDeleted,
    /// Photos with picks.
    Picks,
}

impl LibrarySource {
    pub fn to_filter(&self, f: &Filter, cat: &Catalog) -> Filter {
        let mut f = f.clone();
        match self {
            LibrarySource::All => {}
            LibrarySource::RecentlyAdded => {
                let latest = cat.photos().map(|p| p.imported.clone()).max().unwrap_or_default();
                f.imported = Some(latest.get(..10).unwrap_or("").to_string());
            }
            LibrarySource::Album(a) => f.album = Some(*a),
            LibrarySource::RecentlyDeleted => f.deleted = true,
            LibrarySource::Picks => f.flag = Some(lightcraft_catalog::Flag::Pick),
        }
        f
    }

    pub fn label(&self, cat: &Catalog) -> String {
        match self {
            LibrarySource::All => "All Photos".into(),
            LibrarySource::RecentlyAdded => "Recently Added".into(),
            LibrarySource::Album(a) => cat.album(*a).map(|a| a.name.clone()).unwrap_or_else(|| "Album".into()),
            LibrarySource::RecentlyDeleted => "Recently Deleted".into(),
            LibrarySource::Picks => "Picks".into(),
        }
    }
}

/// Selected photos (ordered) and the active (most-selected) one.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Selection {
    pub ids: Vec<PhotoId>,
    pub active: Option<PhotoId>,
}

impl Selection {
    pub fn single(id: PhotoId) -> Selection {
        Selection { ids: vec![id], active: Some(id) }
    }
    pub fn contains(&self, id: PhotoId) -> bool {
        self.ids.contains(&id)
    }
    pub fn toggle(&mut self, id: PhotoId) {
        if let Some(i) = self.ids.iter().position(|x| *x == id) {
            self.ids.remove(i);
            if self.active == Some(id) {
                self.active = self.ids.last().copied();
            }
        } else {
            self.ids.push(id);
            self.active = Some(id);
        }
    }
    /// Shift-click: select the range between the active photo and `id` in `order`.
    pub fn extend_to(&mut self, id: PhotoId, order: &[PhotoId]) {
        let anchor = self.active.unwrap_or(id);
        let (Some(a), Some(b)) = (order.iter().position(|x| *x == anchor), order.iter().position(|x| *x == id)) else {
            *self = Selection::single(id);
            return;
        };
        let (lo, hi) = (a.min(b), a.max(b));
        for x in &order[lo..=hi] {
            if !self.ids.contains(x) {
                self.ids.push(*x);
            }
        }
        self.active = Some(id);
    }
}
