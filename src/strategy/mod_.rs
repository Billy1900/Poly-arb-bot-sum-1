use crate::types::{GlobalSnapshot, OrderIntent};

pub trait Strategy: Send + Sync {
    fn on_snapshot(&self, snap: &GlobalSnapshot) -> Vec<OrderIntent>;
}
