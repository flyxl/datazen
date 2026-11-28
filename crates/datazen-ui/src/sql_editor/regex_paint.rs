//! Passive Find decoration is a separate, latest-only channel. It never applies
//! edits or selections and cannot cancel, starve or accidentally consume a
//! user-issued Find/Replace command. No regex engine runs during a view build.
use std::{hash::{DefaultHasher, Hash, Hasher}, ops::Range, rc::Rc, time::Duration};
use datazen_ui_api::native_query_regex::{self as api, projection::Projection, Error};
use rune::{el, El, Tx};
use crate::{query_editor_tabs::QueryEditorViewport, AppState};
use super::{find_paint::FindPaint, SqlEditor};

#[derive(Debug)]
struct Stamp {
    identity: Rc<()>,
    rev: u64,
    doc: String,
    pattern: String,
    case_sensitive: bool,
    whole_word: bool,
    window: Range<usize>,
}
impl Stamp {
    fn active(&self,e:&SqlEditor)->bool {
        e.find_open && e.find_regex && e.goto_lease.is_none() && e.composition().is_none()
            && Rc::ptr_eq(&self.identity,&e.identity) && self.rev==e.document_revision
            && self.doc==e.doc() && self.pattern==e.find_input.text
            && self.case_sensitive==e.find_match_case && self.whole_word==e.find_whole_word
    }
    fn request(&self,id:u64)->Result<api::MarkRequest,Error> {
        let doc=Projection::new(&self.doc)?;
        Ok(api::MarkRequest { id,viewport:doc.to_normalized(self.window.clone())?,
            document:doc.normalized().to_owned(),pattern:self.pattern.clone(),
            case_sensitive:self.case_sensitive,whole_word:self.whole_word })
    }
    fn map(&self,marks:Vec<Range<usize>>)->Result<Vec<Range<usize>>,Error> {
        let doc=Projection::new(&self.doc)?;
        api::check_marks(doc.normalized(),&marks)?;
        marks.into_iter().map(|r|doc.to_original(r)).collect()
    }
}
#[derive(Debug)]
pub(super) struct Cached { key:Rc<Stamp>, marks:Vec<Range<usize>> }
#[derive(Default)]
pub(super) struct Controller { last_id:u64, pending:Option<Pending> }
struct Pending {
    id:u64, key:Rc<Stamp>, owner:QueryEditorViewport, session:Option<u32>,
    not_before:f64, deadline:f64, sent:bool,
}

pub(super) fn mounted(e:&SqlEditor, owner:Option<&QueryEditorViewport>, window:Option<Range<usize>>,dark:bool)->(Option<FindPaint>,Option<El>) {
    let (Some(owner),Some(window))=(owner,window) else {return (None,None)};
    if e.composition().is_some() {return (None,None)}
    if let Some(cache)=&e.regex_marks {
        if cache.key.window == window && cache.key.active(e) {
            return (FindPaint::from_ranges(e,cache.marks.iter().cloned(),dark),None)
        }
    }
    let owner=owner.clone();
    let key=Rc::new(Stamp { identity:e.identity.clone(),rev:e.document_revision,doc:e.doc().to_owned(),
        pattern:e.find_input.text.clone(),case_sensitive:e.find_match_case,whole_word:e.find_whole_word,window });
    // Rune has no mount effect. A zero-size pinned, noninteractive leaf sends
    // the exact measured view to the model on first layout, including scroll-
    // only or resize-only invalidation. Never run transport in the builder.
    let mut hash=DefaultHasher::new();
    (&owner.anchor,key.rev,&key.pattern,key.case_sensitive,key.whole_word,&key.window).hash(&mut hash);
    let probe=el().key(format!("sql-regex-layout-{:x}",hash.finish()))
        .pin(Some(0.0), Some(0.0),None,None).size(0.0,0.0)
        .on_layout_tx(move |s:&mut AppState,_rect:[f32;4],tx:&mut Tx| {
            if owner.owns_current_query(s) && key.active(&s.sql) {
                s.stage_regex_marks(owner.clone(),key.clone(),tx);
            }
        });
    (None,Some(probe))
}

impl AppState {
    pub(crate) fn cancel_regex_marks(&mut self) {
        if let Some(p)=self.query_regex.paints.pending.take() {
            if p.sent { self.desktop.cancel_native_regex_marks(p.id); }
        }
        self.sql.regex_marks=None;
    }
    fn stage_regex_marks(&mut self,owner:QueryEditorViewport,key:Rc<Stamp>,tx:&mut Tx) {
        if self.sql.regex_marks.as_ref().is_some_and(|c|c.key.window==key.window && c.key.active(&self.sql)) { return }
        self.cancel_regex_marks();
        let Some(id)=self.query_regex.paints.last_id.checked_add(1) else{return};
        self.query_regex.paints.last_id=id;
        // Installing the lease immediately removes old decorations and coalesces
        // repeated layouts (or a refused/timeout response) for this exact key.
        self.sql.regex_marks=Some(Cached{ key:key.clone(),marks:Vec::new() });
        self.query_regex.paints.pending=Some(Pending { id,key,owner,session:self.navigator_value_search_session_generation,
            not_before:tx.clock()+0.06,deadline:tx.clock()+2.0,sent:false });
        tx.wake_after(Duration::from_millis(60));
    }
    pub(crate) fn poll_regex_marks(&mut self,now:f64,tx:&mut Tx)->bool {
        let Some(p)=self.query_regex.paints.pending.as_ref() else{return false};
        if !p.owner.owns_current_query(self) || !p.key.active(&self.sql)
            || self.sql.regex_marks.as_ref().is_none_or(|c|!Rc::ptr_eq(&c.key,&p.key))
            || p.session!=self.navigator_value_search_session_generation {
            self.cancel_regex_marks();return true;
        }
        let p=self.query_regex.paints.pending.as_mut().unwrap();
        if now>=p.deadline {
            if p.sent { self.desktop.cancel_native_regex_marks(p.id); }
            self.query_regex.paints.pending=None;
            return false;
        }
        if now<p.not_before {tx.wake_after(Duration::from_secs_f64((p.not_before-now).min(0.1)));return false}
        if !p.sent {
            let sent=p.key.request(p.id).and_then(|r|self.desktop.begin_native_regex_marks(r));
            if sent.is_err() {self.query_regex.paints.pending=None;return false}
            p.sent=true;
        }
        if let Some(d)=self.desktop.poll_native_regex_marks().filter(|d|d.id==p.id) {
            let marks=d.result.and_then(|r|p.key.map(r)).unwrap_or_default();
            // A failed or over-budget scan intentionally produces no partial
            // decoration. It never fabricates a literal match or command error.
            self.sql.regex_marks=Some(Cached{key:p.key.clone(),marks});
            self.query_regex.paints.pending=None;
            true
        } else {tx.wake_after(Duration::from_millis(20));false}
    }
}
