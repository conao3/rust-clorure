use crate::clorure::runtime::value::Value;

pub trait ISeq {
    fn first(&self) -> Option<&Value>;
    fn rest(&self) -> Vec<Value>;
}

pub trait Seqable {
    fn seq(&self) -> Option<Vec<Value>>;
}

pub trait Counted {
    fn count(&self) -> usize;
}
