//! The mounted Find commands submit only an immutable, bounded draft. A
//! completion has no authority to focus the editor or edit a different tab.
use std::{ops::Range, rc::Rc, time::Duration};
use datazen_ui_api::native_query_regex::{self as api, projection::Projection, Edit, Error, Operation, Outcome};
use rune::Tx;
use crate::{AppState, query_editor_tabs::QueryEditorViewport};
use super::{Selection, Sel, SqlEditor};

#[derive(Default)]
pub(crate) struct Controller { last_id: u64, pending: Option<Pending>, pub(super) paints: super::regex_paint::Controller }
struct Pending {
    from_canvas:bool, id:u64, owner:QueryEditorViewport, op:Operation, lease:Lease, deadline:f64, session:Option<u32>,
}
struct Lease {
    identity:Rc<()>, revision:u64, doc:String, selection:Selection,
    pattern:String, replacement:String, case_sensitive:bool, whole_word:bool,
}
impl Lease {
    fn capture(e:&SqlEditor) -> Result<Self,Error> {
        if e.doc().len() > api::MAX_DOC || e.find_input.text.len() > api::MAX_PATTERN
            || e.replace_input.text.len() > api::MAX_TEMPLATE || e.selection.len() > api::MAX_RANGES { return Err(Error::Limit); }
        if e.find_input.text.is_empty() { return Err(Error::Empty); }
        if e.selection.ranges.get(e.selection.main).is_none() { return Err(Error::UnsupportedRange); }
        Ok(Self { identity:e.identity.clone(),revision:e.document_revision,doc:e.doc.clone(),selection:e.selection.clone(),
            pattern:e.find_input.text.clone(),replacement:e.replace_input.text.clone(),case_sensitive:e.find_match_case,whole_word:e.find_whole_word })
    }
    fn request(&self, id: u64, op: Operation) -> Result<api::Request, Error> {
        let doc = Projection::new(&self.doc)?;
        // A compound selection must not smuggle an edge inside a CRLF pair
        // into the persisted undo state or an atomic replacement.
        for r in &self.selection.ranges { doc.to_normalized(r.from()..r.to())?; }
        let main = self.selection.main();
        Ok(api::Request {
            id, operation: op, selection: doc.to_normalized(main.from()..main.to())?,
            document: doc.normalized().to_owned(), pattern: self.pattern.clone(),
            replacement: if op.replacing() { self.replacement.clone() } else { String::new() },
            case_sensitive: self.case_sensitive, whole_word: self.whole_word,
        })
    }
    fn project(&self, current: &str, op: Operation, out: Outcome) -> Result<Option<Outcome>, Error> {
        let doc = Projection::new(&self.doc)?;
        let main = self.selection.main();
        doc.checked_outcome(current, op, main.from()..main.to(), out)
    }
    fn owns(&self,e:&SqlEditor)->bool {
        e.find_regex && e.find_open && e.goto_lease.is_none() && e.composition().is_none()
            && Rc::ptr_eq(&self.identity,&e.identity) && self.revision==e.document_revision && self.doc==e.doc
            && self.selection==e.selection && self.pattern==e.find_input.text && self.replacement==e.replace_input.text
            && self.case_sensitive==e.find_match_case && self.whole_word==e.find_whole_word
    }
}
impl AppState {
    pub(crate) fn cancel_query_regex(&mut self) {
        if let Some(p)=self.query_regex.pending.take() { self.desktop.cancel_native_query_regex(p.id); }
        self.sql.regex_error=None;
    }
    pub(super) fn begin_query_regex(&mut self,op:Operation,tx:&mut Tx) {
        self.cancel_query_regex();
        let Some(owner)=self.query_editor_viewport().filter(|o| o.owns_current_query(self)) else {return};
        if !self.sql.find_regex || !self.sql.find_open || self.sql.composition().is_some()
            || self.sql.goto_lease.is_some() || self.query_snippet_menu_owner().is_some() || self.result_focused { return; }
        let lease=match Lease::capture(&self.sql) { Ok(l)=>l,Err(e)=>{self.sql.regex_error=Some(e);return} };
        let Some(id)=self.query_regex.last_id.checked_add(1) else {self.sql.regex_error=Some(Error::Unavailable);return};
        self.query_regex.last_id=id;
        let request=match lease.request(id, op) { Ok(r)=>r,Err(e)=>{self.sql.regex_error=Some(e);return} };
        if let Err(e)=self.desktop.begin_native_query_regex(request){self.sql.regex_error=Some(e);return}
        self.query_regex.pending=Some(Pending{from_canvas:tx.focused()==Some("sql-editor-scroll"),id,op,lease,owner,deadline:tx.clock()+2.0,session:self.navigator_value_search_session_generation});
        tx.wake_after(Duration::from_millis(20));
    }
    pub(crate) fn poll_query_regex(&mut self,now:f64,tx:&mut Tx)->bool {
        let Some(p)=&self.query_regex.pending else{return false};
        // A click must not deliver to a covered editor or re-focus a user
        // who has since moved to the grid, navigator or another window.
        // Kit keys its bound inner TextInput "input" on pointer focus. Never
        // accept that generic key alone: unrelated fields use it too. Rune
        // updates the exact draft's bound TextEdit on both focus and blur.
        let focused_dock_field = tx.focused() == Some("input")
            && (self.sql.find_input.focused || self.sql.replace_input.focused);
        let in_dock=focused_dock_field || tx.focused().is_some_and(|k| matches!(k,"sql-find-input"|"sql-find-next"|"sql-find-prev"|
            "sql-find-select-all-matches"|"sql-find-regex"|"sql-find-match-case"|"sql-find-whole-word"|"sql-replace-input"|
            "sql-replace-current"|"sql-replace-all"|"sql-add-cursor"));
        let focus_owned = if p.from_canvas { tx.focused() == Some("sql-editor-scroll") && self.sql.focused() } else { in_dock };
        if !focus_owned || !p.owner.owns_current_query(self) || !p.lease.owns(&self.sql)
            || p.session!=self.navigator_value_search_session_generation || self.query_snippet_menu_owner().is_some() || self.result_focused {
            self.cancel_query_regex();return false;
        }
        if now >= p.deadline {self.cancel_query_regex();self.sql.regex_error=Some(Error::Timeout);return true}
        if let Some(done)=self.desktop.poll_native_query_regex().filter(|d| d.id==p.id) {
            let Some(p)=self.query_regex.pending.take() else{return false};
            let result=done.result.and_then(|out| p.lease.project(self.sql.doc(),p.op,out))
                .and_then(|mapped| match mapped {
                    Some(out) => self.sql.apply_regex(p.op,out),
                    None => Ok(false),
                });
            match result {
                Ok(true)=>{ super::find_dock::reveal_selection(&self.sql,tx); true },
                Ok(false)=>false,Err(e)=>{self.sql.regex_error=Some(e);true}
            }
        } else {
            tx.wake_after(Duration::from_millis(20));false
        }
    }
}

pub(super) fn status(e:Error)->Option<&'static str> {
    Some(match e {
        Error::Empty=>return None,Error::InvalidPattern=>"query.nativeFind.invalidRegex",
        Error::UnsupportedRange=>"query.nativeFind.regexRange",Error::Limit=>"query.nativeFind.regexLimit",
        Error::Timeout=>"query.nativeFind.regexTimeout",Error::InvalidReply|Error::Unavailable=>"query.nativeFind.regexUnavailable",
    })
}

// ChangeSet.mapPos for a checked, ascending list of edits, in UTF-8 bytes.
// A zero-width insertion at the edge of a range honors the edge's bias.
struct Plan<'a> { edits:&'a [Edit], ends:Vec<usize> }
impl<'a> Plan<'a> {
    fn new(edits:&'a [Edit])->Self {
        let (mut end,mut output)=(0,0);
        let ends=edits.iter().map(|e| {output+=e.range.start-end+e.insert.len();end=e.range.end;output}).collect();
        Self{edits,ends}
    }
    fn map(&self,at:usize,right:bool)->usize {
        let i=self.edits.partition_point(|e| e.range.end < at || (e.range.end==at && (!e.range.is_empty() || right)));
        let (source,target)=if i==0 {(0,0)}else{(self.edits[i-1].range.end,self.ends[i-1])};
        if let Some(e)=self.edits.get(i).filter(|e| e.range.start<=at) {
            let start=target+(e.range.start-source);
            if right && at>e.range.start { start+e.insert.len() } else { start }
        } else { target+(at-source) }
    }
    fn map_marker(&self,at:usize)->Option<usize> {
        let i=self.edits.partition_point(|e| e.range.end<=at);
        if self.edits.get(i).is_some_and(|e|e.range.contains(&at)) {None}else{Some(self.map(at,true))}
    }
    fn range(&self,s:Sel)->Result<Sel,Error> {
        if s.anchor==s.head {return Ok(Sel::caret(self.map(s.head,false)))}
        let (a,b)=(self.map(s.from(),true),self.map(s.to(),false));
        // CM's internal ordered edges may invert when a replacement fully
        // covers an arbitrary selected subrange. Refuse rather than invent
        // an affinity for a Native selection that cannot represent it.
        if a>b {return Err(Error::UnsupportedRange)}
        Ok(if s.anchor<s.head {Sel{anchor:a,head:b}} else {Sel{anchor:b,head:a}})
    }
}
fn sel(r:Range<usize>)->Sel { Sel {anchor:r.start, head:r.end} }
impl SqlEditor {
    fn apply_regex(&mut self,op:Operation,out:Outcome)->Result<bool,Error> {
        let main=self.selection.main();
        out.check(op,self.doc(),main.from()..main.to())?;
        let selected=match out {
            Outcome::Select(ranges) => {
                if ranges.is_empty(){return Ok(false)}
                super::select_next_occurrence::normalized(ranges.into_iter().enumerate().map(|(i,r)|(sel(r),i==0)).collect()).ok_or(Error::InvalidReply)?
            }
            Outcome::Edits{edits,following}=>{
                let plan=Plan::new(&edits);
                let mapped=if let Some(r)=following {vec![(plan.range(sel(r))?,true)]} else {
                    self.selection.ranges.iter().enumerate().map(|(i,s)|Ok((plan.range(*s)?,i==self.selection.main))).collect::<Result<Vec<_>,Error>>()?
                };
                let after=super::select_next_occurrence::normalized(mapped).ok_or(Error::InvalidReply)?;
                let mut updated=String::with_capacity(plan.map(self.doc.len(),true));
                let mut at=0;
                for e in &edits {updated.push_str(&self.doc[at..e.range.start]);updated.push_str(&e.insert);at=e.range.end;}
                updated.push_str(&self.doc[at..]);
                if after.ranges.iter().any(|s| updated.get(s.from()..s.to()).is_none()) {return Err(Error::InvalidReply)}
                if updated!=self.doc {
                    self.push_history();
                    self.bracket_closers.map_batch(|at|plan.map_marker(at));
                    self.doc=updated;
                    self.document_revision=self.document_revision.wrapping_add(1);
                }
                after
            }
        };
        self.selection=selected;
        self.preferred_columns.clear();self.completion_close();self.keep_bracket_closers_on_main_line();
        Ok(true)
    }
}
