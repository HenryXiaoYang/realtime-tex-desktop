// The sidebar's side of a document: the element at the caret or selected (else the document),
// its fields, and setting one as an undo step, with the packages it needs.
use super::*;
use document::element::{self, Element, Field};

impl Doc {
    /// The element the sidebar shows: the selected block's, the caret's, else the document.
    pub fn element(&self) -> Element {
        if self.active.is_none()
            && let Some((lo, _)) = self.selection()
        {
            let b = self.blocks[lo].range.clone();
            return element::element_at(&self.source, Some(b.clone()), b.start);
        }
        let Some(c) = self.caret.as_ref().filter(|_| self.active.is_none()) else {
            return element::element_at(&self.source, None, 0);
        };
        let block = self.block_of(c.off).map(|i| self.blocks[i].range.clone());
        element::element_at(&self.source, block, c.off)
    }

    /// The element and its fields now.
    pub fn element_fields(&self) -> (Element, Vec<Field>) {
        let el = self.element();
        let fields = element::fields(&self.source, &el);
        (el, fields)
    }

    /// Give field `key` of the element at hand `value`, as an undo step.
    pub fn set_element_field(&mut self, key: &str, value: &str, cx: &mut Context<Self>) {
        let el = self.element();
        let Some(change) = element::set(&self.source, &el, key, value) else {
            return;
        };
        if self.source.get(change.range.clone()) == Some(change.text.as_str()) {
            return;
        }
        self.checkpoint();
        self.typing = None;
        self.overlay = None;
        // the selection and caret stay on the element (they follow the edit)
        self.replace(change.range.clone(), &change.text, cx);
        self.add_packages(&change.packages, cx);
        cx.notify();
    }
}
