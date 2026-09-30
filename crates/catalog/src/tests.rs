use std::sync::Arc;

use lightcraft_develop::DevelopSettings;

use super::*;

fn photo(c: &mut Catalog, name: &str, date: &str) -> PhotoId {
    let id = c.alloc_photo_id();
    let mut p = Photo::new(id, Source::Demo { scene: 1 }, name, "JPEG", 6000, 4000, "2026-09-30T10:00:00");
    p.captured = Some(date.to_string());
    c.apply(Op::AddPhoto { photo: Box::new(p) }).unwrap();
    id
}

#[test]
fn apply_and_inverse_roundtrip() {
    let mut c = Catalog::new();
    let a = photo(&mut c, "a.jpg", "2026-04-01T10:00:00");
    let before = c.clone();
    let ops = vec![
        Op::SetRating { id: a, rating: 4 },
        Op::SetFlag { id: a, flag: Flag::Pick },
        Op::SetLabel { id: a, label: Some(ColorLabel::Red) },
        Op::SetDevelop {
            id: a,
            settings: Arc::new(DevelopSettings { treatment: lightcraft_develop::Treatment::Bw, ..Default::default() }),
            label: "B&W".into(),
            edited: Some("x".into()),
        },
    ];
    let mut invs = Vec::new();
    for op in ops {
        invs.push(c.apply(op).unwrap());
    }
    assert_eq!(c.photo(a).unwrap().rating, 4);
    for inv in invs.into_iter().rev() {
        c.apply(inv).unwrap();
    }
    assert_eq!(c.photos().collect::<Vec<_>>(), before.photos().collect::<Vec<_>>());
}

#[test]
fn invalid_ops_change_nothing() {
    let mut c = Catalog::new();
    let a = photo(&mut c, "a.jpg", "2026-04-01");
    let snap = c.to_snapshot();
    assert!(c.apply(Op::SetRating { id: a, rating: 9 }).is_err());
    assert!(c.apply(Op::SetRating { id: PhotoId(999), rating: 1 }).is_err());
    // batch with a failing op rolls back
    let r = c.apply(Op::Batch { ops: vec![Op::SetRating { id: a, rating: 2 }, Op::SetFlag { id: PhotoId(77), flag: Flag::Pick }] });
    assert!(r.is_err());
    assert_eq!(c.to_snapshot(), snap);
}

#[test]
fn albums_and_folders() {
    let mut c = Catalog::new();
    let a = photo(&mut c, "a.jpg", "2026-04-01");
    let f = c.alloc_album_id();
    c.apply(Op::AddAlbum { album: Album { id: f, name: "Trips".into(), parent: None, folder: true, photos: vec![], cover: None } }).unwrap();
    let al = c.alloc_album_id();
    c.apply(Op::AddAlbum { album: Album { id: al, name: "Alps".into(), parent: Some(f), folder: false, photos: vec![a], cover: None } }).unwrap();
    assert_eq!(c.albums_of(a), vec![al]);
    assert!(c.apply(Op::RemoveAlbum { id: f }).is_err(), "non-empty folder");
    assert!(c.apply(Op::SetAlbumPhotos { id: f, photos: vec![a] }).is_err(), "folders hold no photos");
    assert!(c.apply(Op::MoveAlbum { id: f, parent: Some(f) }).is_err());
    let del = c.delete_permanently_ops(a);
    let inv = c.apply(del).unwrap();
    assert!(c.photo(a).is_none() && c.album(al).unwrap().photos.is_empty());
    c.apply(inv).unwrap();
    assert!(c.photo(a).is_some() && c.album(al).unwrap().photos == vec![a]);
}

#[test]
fn filter_search_sort() {
    let mut c = Catalog::new();
    let a = photo(&mut c, "beach.jpg", "2026-05-01T10:00:00");
    let b = photo(&mut c, "alps.jpg", "2026-04-01T10:00:00");
    let d = photo(&mut c, "city.jpg", "2025-12-24T10:00:00");
    c.apply(Op::SetRating { id: b, rating: 5 }).unwrap();
    c.apply(Op::SetFlag { id: d, flag: Flag::Reject }).unwrap();
    let mut m = c.photo(a).unwrap().meta.clone();
    m.keywords = vec!["ocean".into(), "summer".into()];
    m.iso = Some(1600);
    c.apply(Op::SetMeta { id: a, meta: Box::new(m) }).unwrap();

    let all = c.query(&Filter::default(), &Sort::default());
    assert_eq!(all, vec![a, b, d], "newest first");
    let asc = c.query(&Filter::default(), &Sort { key: SortKey::FileName, ascending: true });
    assert_eq!(asc, vec![b, a, d]);
    assert_eq!(c.query(&Filter { rating: 4, ..Default::default() }, &Sort::default()), vec![b]);
    assert_eq!(c.query(&Filter { flag: Some(Flag::Reject), ..Default::default() }, &Sort::default()), vec![d]);
    assert_eq!(c.query(&Filter { text: "summer".into(), ..Default::default() }, &Sort::default()), vec![a]);
    assert_eq!(c.query(&Filter { text: "iso:>800".into(), ..Default::default() }, &Sort::default()), vec![a]);
    assert_eq!(c.query(&Filter { text: "date:2025".into(), ..Default::default() }, &Sort::default()), vec![d]);
    assert_eq!(c.query(&Filter { date: Some("2026-04".into()), ..Default::default() }, &Sort::default()), vec![b]);
    c.apply(Op::SetDeleted { id: d, deleted: true }).unwrap();
    assert_eq!(c.query(&Filter::default(), &Sort::default()).len(), 2);
    assert_eq!(c.query(&Filter { deleted: true, ..Default::default() }, &Sort::default()), vec![d]);
    let g = c.date_groups();
    assert_eq!(g[0].year, "2026");
    assert_eq!(g[0].count, 2);
    assert_eq!(c.keywords(), vec![("ocean".to_string(), 1), ("summer".to_string(), 1)]);
}

#[test]
fn snapshot_and_log_replay() {
    let mut c = Catalog::new();
    let snap0 = c.to_snapshot();
    let mut log = String::new();
    let a = c.alloc_photo_id();
    let ops = vec![
        Op::AddPhoto { photo: Box::new(Photo::new(a, Source::File { path: "/x/a.jpg".into() }, "a.jpg", "JPEG", 10, 10, "t")) },
        Op::SetRating { id: a, rating: 3 },
        Op::SetFlag { id: a, flag: Flag::Pick },
    ];
    for op in ops {
        log.push_str(&Catalog::op_to_log_line(&op));
        c.apply(op).unwrap();
    }
    let mut r = Catalog::from_snapshot(&snap0).unwrap();
    // torn last line is tolerated
    let torn = format!("{log}{{\"op\":\"setRat");
    assert_eq!(r.replay(&torn).unwrap(), 3);
    assert_eq!(r.to_snapshot(), c.to_snapshot());
    assert!(Catalog::from_snapshot("{nope").is_err());
}

proptest::proptest! {
    #[test]
    fn random_ops_undo_to_start(ratings in proptest::collection::vec((0usize..3, 0u8..6, 0u8..3), 1..40)) {
        let mut c = Catalog::new();
        let ids: Vec<PhotoId> = (0..3).map(|i| photo(&mut c, &format!("p{i}.jpg"), "2026-01-01")).collect();
        let start = c.to_snapshot();
        let mut invs = Vec::new();
        for (i, r, f) in ratings {
            let op = if f == 0 { Op::SetRating { id: ids[i], rating: r } } else { Op::SetFlag { id: ids[i], flag: [Flag::None, Flag::Pick, Flag::Reject][f as usize] } };
            if let Ok(inv) = c.apply(op) { invs.push(inv); }
        }
        for inv in invs.into_iter().rev() { c.apply(inv).unwrap(); }
        proptest::prop_assert_eq!(c.to_snapshot(), start);
    }
}
