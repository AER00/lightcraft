//! Organizing commands: stacks (group, ungroup, set top, expand/collapse, auto-stack by capture
//! time).

use lightcraft_catalog::{Op, PhotoId, StackId};
use serde_json::{Value, json};

use super::{CommandSpec, always, bad, bool_or, cmd, f64_or, has_selection};
use crate::{Result, Selection, Session};

/// Stacks containing any of `targets`, in first-seen order.
fn stacks_of(s: &Session, targets: &[PhotoId]) -> Vec<StackId> {
    let mut v = Vec::new();
    for t in targets {
        if let Some(st) = s.catalog.stack_of(*t)
            && !v.contains(&st.id)
        {
            v.push(st.id);
        }
    }
    v
}

fn in_stack(s: &Session) -> std::result::Result<(), String> {
    has_selection(s)?;
    if stacks_of(s, &s.targets(&Value::Null)).is_empty() { Err("no stacked photo selected".into()) } else { Ok(()) }
}

fn has_stacks(s: &Session) -> std::result::Result<(), String> {
    if s.catalog.stacks().next().is_none() { Err("no stacks".into()) } else { Ok(()) }
}

fn commit_ops(s: &mut Session, label: &str, ops: Vec<Op>) -> Result<usize> {
    let n = ops.len();
    if n > 0 {
        s.commit(label, Op::Batch { ops })?;
    }
    Ok(n)
}

/// After collapsing, keep the active photo visible (move it to its stack's top).
fn fix_active(s: &mut Session) {
    let vis = s.visible_cloned();
    if let Some(a) = s.selection.active
        && !vis.contains(&a)
        && let Some(st) = s.catalog.stack_of(a)
    {
        let top = vis.iter().copied().find(|p| st.photos.contains(p)).unwrap_or(st.top());
        s.selection = Selection::single(top);
    }
}

fn set_collapsed(s: &mut Session, stacks: Vec<StackId>, collapsed: bool, label: &str) -> Result<Value> {
    let ops = s.catalog.set_collapsed_ops(&stacks, collapsed);
    let n = commit_ops(s, label, ops)?;
    if collapsed {
        fix_active(s);
    }
    Ok(json!({"changed": n}))
}

pub fn specs() -> Vec<CommandSpec> {
    vec![
        cmd!(
            "stack.group",
            "Group into Stack",
            ["Photo", "Stack"],
            Some("Cmd+G"),
            "{ids?, top?: photoId (default: the active photo), collapsed?: bool (default true)} — stacks the photos are already in are merged",
            has_selection,
            |s, p| {
                let targets = s.targets(p);
                let top = p
                    .get("top")
                    .and_then(Value::as_u64)
                    .map(PhotoId)
                    .or(s.selection.active.filter(|a| targets.contains(a)))
                    .or(targets.first().copied())
                    .ok_or_else(|| bad("stack.group", "no photos"))?;
                let op = s
                    .catalog
                    .group_ops(top, &targets, bool_or(p, "collapsed", true))
                    .ok_or_else(|| bad("stack.group", "select at least two photos"))?;
                s.commit("Group into Stack", op)?;
                let st = s.catalog.stack_of(top).map(|st| (st.id.0, st.photos.len())).unwrap_or_default();
                s.selection = Selection::single(top);
                Ok(json!({"stack": st.0, "count": st.1}))
            }
        ),
        cmd!("stack.ungroup", "Ungroup Stack", ["Photo", "Stack"], Some("Cmd+Shift+G"), "{ids?}", in_stack, |s, p| {
            let ops = s.catalog.ungroup_ops(&s.targets(p));
            Ok(json!({"ungrouped": commit_ops(s, "Ungroup Stack", ops)?}))
        }),
        cmd!("stack.remove", "Remove from Stack", ["Photo", "Stack"], None, "{ids?}", in_stack, |s, p| {
            let ops = s.catalog.remove_from_stacks_ops(&s.targets(p));
            Ok(json!({"changed": commit_ops(s, "Remove from Stack", ops)?}))
        }),
        cmd!("stack.setTop", "Set as Top of Stack", ["Photo", "Stack"], Some("Shift+S"), "{id?} (default: the active photo)", in_stack, |s, p| {
            let id = p.get("id").and_then(Value::as_u64).map(PhotoId).or(s.active()).ok_or_else(|| bad("stack.setTop", "no photo"))?;
            let op = s.catalog.set_top_ops(id);
            Ok(json!({"changed": commit_ops(s, "Set Top of Stack", op.into_iter().collect())?}))
        }),
        cmd!("stack.toggle", "Expand/Collapse Stack", ["Photo", "Stack"], Some("S"), "{ids?, collapsed?: bool}", in_stack, |s, p| {
            let stacks = stacks_of(s, &s.targets(p));
            let first = stacks.first().and_then(|id| s.catalog.stack(*id)).map(|st| st.collapsed).unwrap_or(false);
            let collapsed = bool_or(p, "collapsed", !first);
            set_collapsed(s, stacks, collapsed, if collapsed { "Collapse Stack" } else { "Expand Stack" })
        }),
        cmd!("stack.expandAll", "Expand All Stacks", ["View", "Stacks"], None, "{}", has_stacks, |s, _| {
            let all: Vec<StackId> = s.catalog.stacks().map(|st| st.id).collect();
            set_collapsed(s, all, false, "Expand All Stacks")
        }),
        cmd!("stack.collapseAll", "Collapse All Stacks", ["View", "Stacks"], None, "{}", has_stacks, |s, _| {
            let all: Vec<StackId> = s.catalog.stacks().map(|st| st.id).collect();
            set_collapsed(s, all, true, "Collapse All Stacks")
        }),
        cmd!(
            "stack.auto",
            "Auto-Stack by Capture Time",
            [],
            None,
            "{gap?: seconds between consecutive captures (default 60), ids? (default: the selection if several photos, else the current view), preview?: bool} → {stacks, photos}",
            always,
            |s, p| {
                let gap = f64_or(p, "gap", 60.0).max(0.0);
                let explicit = p.get("ids").is_some();
                let pool = if explicit || s.selection.ids.len() > 1 { s.targets(p) } else { s.visible_cloned() };
                let groups = s.catalog.auto_stack_groups(&pool, gap);
                let photos: usize = groups.iter().map(Vec::len).sum();
                if bool_or(p, "preview", false) {
                    return Ok(json!({"stacks": groups.len(), "photos": photos}));
                }
                let mut ops = Vec::new();
                for g in &groups {
                    ops.extend(s.catalog.group_ops(g[0], g, true));
                }
                commit_ops(s, "Auto-Stack", ops)?;
                fix_active(s);
                Ok(json!({"stacks": groups.len(), "photos": photos}))
            }
        ),
    ]
}
