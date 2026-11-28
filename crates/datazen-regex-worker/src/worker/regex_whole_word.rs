//! Exactly the CodeMirror state 6.7.4 scalar categorizer and the directional
//! Original find-cluster-break MIT notice: find-cluster-break-MIT.txt.
//! cluster boundary walk from find-cluster-break 1.0.4, evaluated on the
//! engine's actual match.input (not the editor document or literal cursor's
//! fallback/clipped context). Stock SQL supplies no lang-dependent wordChars.
use super::{regex_cursor::Error, whole_word_data::{EXTEND, WORD}};
use std::ops::Range;

const ZWJ: u32 = 0x200d;
fn high(u: u16) -> bool { (0xd800..0xdc00).contains(&u) }
fn low(u: u16) -> bool { (0xdc00..0xe000).contains(&u) }
fn regional(cp: u32) -> bool { (0x1f1e6..=0x1f1ff).contains(&cp) }
fn has(table: &[(u32,u32)], cp: u32) -> bool {
    let slot = table.partition_point(|&(_,end)| end<=cp);
    table.get(slot).is_some_and(|&(start,_)|start<=cp)
}

pub(super) struct WordBudget(pub(super) usize);
impl WordBudget {
    fn step(&mut self) -> Result<(), Error> {
        self.0 = self.0.checked_sub(1).ok_or(Error::Limit)?;
        Ok(())
    }
    fn scalar(&mut self, input: &[u16], i: usize) -> Result<(u32,usize), Error> {
        self.step()?;
        let a=input[i];
        if high(a) && let Some(&b)=input.get(i+1).filter(|&&b|low(b)) {
            return Ok((0x1_0000+((u32::from(a)-0xd800)<<10)+u32::from(b)-0xdc00,2));
        }
        Ok((u32::from(a),1))
    }
    fn after(&mut self, input: &[u16], at: usize) -> Result<usize, Error> {
        if at==input.len() {return Ok(at);}
        let mut pos=if at>0 && low(input[at]) && high(input[at-1]) {at-1} else {at};
        let (mut prior,width)=self.scalar(input,pos)?;
        pos+=width;
        while pos<input.len() {
            let (next,width)=self.scalar(input,pos)?;
            if prior==ZWJ || next==ZWJ || has(EXTEND,next) {
                pos+=width;
                prior=next;
            } else if regional(next) {
                let mut before=pos.checked_sub(2);
                let mut odd=false;
                while let Some(i)=before {
                    if !regional(self.scalar(input,i)?.0) {break;}
                    odd=!odd;
                    before=i.checked_sub(2);
                }
                if !odd {break;}
                pos+=2;
            } else {break;}
        }
        Ok(pos)
    }
    fn before(&mut self, input: &[u16], mut pos: usize) -> Result<usize,Error> {
        while pos>1 {
            let found=self.after(input,pos-2)?;
            if found<pos {return Ok(found);}
            pos-=1;
        }
        Ok(0)
    }
    fn word(&mut self, input: &[u16], mut at: usize, end: usize) -> Result<bool,Error> {
        while at<end {
            let (cp,width)=self.scalar(input,at)?;
            if at+width<=end && has(WORD,cp) {return Ok(true);}
            at+=width;
        }
        Ok(false)
    }
    fn connected(&mut self, input: &[u16], edge: usize) -> Result<bool,Error> {
        let before=self.before(input,edge)?;
        if !self.word(input,before,edge)? {return Ok(false);}
        let after=self.after(input,edge)?;
        self.word(input,edge,after)
    }

    pub(super) fn accepts(&mut self, input: &[u16], hit: Range<usize>) -> Result<bool,Error> {
        if hit.start>hit.end || hit.end>input.len() {return Err(Error::Bounds);}
        // CM explicitly bypasses all categorical checks for zero-width hits.
        if hit.is_empty() {return Ok(true);}
        Ok(!self.connected(input,hit.start)? && !self.connected(input,hit.end)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn contextual_boundaries_and_zero_length() {
        let sample: Vec<_> = "猫cat 𞛀cat 🐈cat e\u{301}cat".encode_utf16().collect();
        let mut filter=WordBudget(1000);
        assert_eq!(filter.accepts(&sample,1..4),Ok(false));
        assert_eq!(filter.accepts(&sample,7..10),Ok(false));
        assert_eq!(filter.accepts(&sample,13..16),Ok(true));
        assert_eq!(filter.accepts(&sample,19..22),Ok(false));
        assert_eq!(filter.accepts(&sample,1..1),Ok(true));
        assert_eq!(filter.accepts(&sample[1..4],0..3),Ok(true));
    }

    #[test]
    fn word_budget_is_shared_and_a_rejection_has_no_edit_fallback() {
        let text: Vec<_> = "xcat".encode_utf16().collect();
        assert_eq!(WordBudget(0).accepts(&text,1..4),Err(Error::Limit));
        assert_eq!(WordBudget(0).accepts(&text,1..1),Ok(true));
    }
}
