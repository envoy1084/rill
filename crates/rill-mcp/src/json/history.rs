//! Repositories, change versions, history walks, refs and landings.

use rill_sdk::{
    BlameLine, LandEvent, LogPage, PathHistoryEntry, PathHistoryPage, RefEvent, RefInfo, RepoInfo,
    VersionResult,
};
use rill_types::objects::change::FileState;
use rill_types::{
    ChangeInfo, ChangeVersion, ContentRef, EntryKind, FileChange, Merge, Provenance, SubtreeChange,
};
use serde_json::{json, Value};

use super::primitives::{enum_name, hash, opt, principal, timestamp};

/// A repository record.
pub fn repo_info(r: &RepoInfo) -> Value {
    json!({
        "id": r.id.to_string(),
        "name": r.name,
        "description": r.description,
        "created_at": timestamp(&r.created_at),
        "trunk_head_seq": r.trunk_head_seq.get(),
    })
}

fn provenance(p: &Provenance) -> Value {
    json!({
        "tool": p.tool,
        "model": p.model,
        "session": p.session,
        "prompt_ref": p.prompt_ref,
        "sponsor": opt(p.sponsor.as_ref(), principal),
        "resolved_by": p.resolved_by,
        "extra": p.extra,
    })
}

/// Change metadata as returned by log and version lookups.
pub fn change_info(c: &ChangeInfo) -> Value {
    json!({
        "hash": hash(&c.hash),
        "change_id": c.change_id.to_string(),
        "parents": c.parents.iter().map(hash).collect::<Vec<_>>(),
        "predecessor": opt(c.predecessor.as_ref(), hash),
        "author": principal(&c.author),
        "provenance": provenance(&c.provenance),
        "intent": c.intent.map(|i| i.to_string()),
        "message": c.message,
        "summary": c.summary(),
        "created_at": timestamp(&c.created_at),
        "files_changed": c.files_changed,
        "has_conflicts": c.has_conflicts,
        "trunk_seq": c.trunk_seq.map(|s| s.get()),
    })
}

fn content_ref(c: &ContentRef) -> Value {
    match c {
        ContentRef::Inline { bytes } => json!({ "kind": "inline", "size": bytes.len() }),
        ContentRef::Blob { hash: h, size } => {
            json!({ "kind": "blob", "hash": hash(h), "size": size })
        }
        ContentRef::Chunked { hash: h, size } => {
            json!({ "kind": "chunked", "hash": hash(h), "size": size })
        }
    }
}

/// A file state: `null` when absent, else content and kind.
pub(super) fn file_state(s: &FileState) -> Value {
    match s {
        None => Value::Null,
        Some((content, kind)) => json!({
            "content": content_ref(content),
            "content_hash": hash(&content.content_hash()),
            "kind": enum_name(kind),
        }),
    }
}

pub(super) fn merge_terms(m: &Merge<FileState>) -> Value {
    json!({
        "adds": m.adds().map(file_state).collect::<Vec<_>>(),
        "removes": m.removes().map(file_state).collect::<Vec<_>>(),
    })
}

/// What a version did to one path.
pub fn file_change(c: &FileChange) -> Value {
    match c {
        FileChange::Set {
            content,
            kind,
            copy_from,
        } => json!({
            "op": "set",
            "content": content_ref(content),
            "content_hash": hash(&content.content_hash()),
            "kind": enum_name(kind),
            "copy_from": copy_from.as_ref().map(|c| json!({
                "path": c.path.as_str(),
                "version": hash(&c.version),
            })),
        }),
        FileChange::Delete => json!({ "op": "delete" }),
        FileChange::Conflict(m) => json!({ "op": "conflict", "terms": merge_terms(m) }),
    }
}

fn subtree_change(c: &SubtreeChange) -> Value {
    match c {
        SubtreeChange::Copy {
            from_path,
            from_version,
        } => {
            json!({ "op": "copy", "from_path": from_path.as_str(), "from_version": hash(from_version) })
        }
        SubtreeChange::Delete => json!({ "op": "delete" }),
    }
}

/// A full change version with its file changes.
pub fn change_version(v: &ChangeVersion) -> Value {
    json!({
        "hash": hash(&v.version_hash()),
        "change_id": v.change_id.to_string(),
        "parents": v.parents.iter().map(hash).collect::<Vec<_>>(),
        "predecessor": opt(v.predecessor.as_ref(), hash),
        "author": principal(&v.author),
        "provenance": provenance(&v.provenance),
        "intent": v.intent.map(|i| i.to_string()),
        "message": v.message,
        "summary": v.summary(),
        "created_at": timestamp(&v.created_at),
        "has_conflicts": v.has_conflicts(),
        "conflicted_paths": v.conflicted_paths().map(|p| p.as_str()).collect::<Vec<_>>(),
        "file_changes": v.file_changes.iter().map(|(p, c)| {
            let mut entry = file_change(c);
            entry["path"] = Value::String(p.to_string());
            entry
        }).collect::<Vec<_>>(),
        "subtree_changes": v.subtree_changes.iter().map(|(p, c)| {
            let mut entry = subtree_change(c);
            entry["path"] = Value::String(p.to_string());
            entry
        }).collect::<Vec<_>>(),
    })
}

/// The hash and version produced by a write.
pub fn version_result(r: &VersionResult) -> Value {
    json!({
        "hash": hash(&r.hash),
        "change_id": r.version.change_id.to_string(),
        "has_conflicts": r.version.has_conflicts(),
        "conflicted_paths": r.conflicted_paths().iter().map(|p| p.as_str()).collect::<Vec<_>>(),
        "version": change_version(&r.version),
    })
}

/// A page of a history walk.
pub fn log_page(p: &LogPage) -> Value {
    json!({
        "entries": p.entries.iter().map(change_info).collect::<Vec<_>>(),
        "next_start": opt(p.next_start.as_ref(), hash),
    })
}

/// One version in a path's history.
pub fn path_history_entry(e: &PathHistoryEntry) -> Value {
    json!({
        "info": change_info(&e.info),
        "path": e.path.as_str(),
        "change": file_change(&e.change),
    })
}

/// A page of a path history walk.
pub fn path_history_page(p: &PathHistoryPage) -> Value {
    json!({
        "entries": p.entries.iter().map(path_history_entry).collect::<Vec<_>>(),
        "next_start": opt(p.next_start.as_ref(), hash),
    })
}

/// One attributed line.
pub fn blame_line(l: &BlameLine) -> Value {
    json!({
        "line_no": l.line_no,
        "version": hash(&l.version),
        "author": principal(&l.author),
        "trunk_seq": l.trunk_seq.map(|s| s.get()),
        "text": String::from_utf8_lossy(&l.text),
    })
}

/// A ref and its target.
pub fn ref_info(r: &RefInfo) -> Value {
    json!({
        "name": r.name.as_str(),
        "target": hash(&r.target),
        "trunk_seq": r.trunk_seq.map(|s| s.get()),
    })
}

/// A ref move.
pub fn ref_event(e: &RefEvent) -> Value {
    json!({
        "name": e.name.as_str(),
        "old": opt(e.old.as_ref(), hash),
        "new": opt(e.new.as_ref(), hash),
        "at": timestamp(&e.at),
    })
}

/// A landing on trunk.
pub fn land_event(e: &LandEvent) -> Value {
    json!({
        "seq": e.seq.get(),
        "hash": hash(&e.hash),
        "change_id": e.change_id.to_string(),
        "landed_by": principal(&e.landed_by),
        "at": timestamp(&e.at),
        "reverts_seq": e.reverts_seq.map(|s| s.get()),
    })
}

/// One directory entry from a listing.
pub fn dir_entry(name: &str, kind: &EntryKind) -> Value {
    match kind {
        EntryKind::File { content, kind } => json!({
            "name": name,
            "type": "file",
            "size": content.size(),
            "kind": enum_name(kind),
            "content_hash": hash(&content.content_hash()),
        }),
        EntryKind::Dir { hash: h, summary } => json!({
            "name": name,
            "type": "dir",
            "hash": hash(h),
            "file_count": summary.file_count,
            "dir_count": summary.dir_count,
            "total_size": summary.total_size,
        }),
    }
}
