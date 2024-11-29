use time::OffsetDateTime;

pub trait Empty {
    fn is_empty(&self) -> bool;
}

pub fn set_true() -> bool {
    true
}

pub fn is_true(value: &bool) -> bool {
    *value
}

pub fn set_false() -> bool {
    false
}

pub fn is_false(value: &bool) -> bool {
    !*value
}

pub fn origin_of_time() -> OffsetDateTime {
    OffsetDateTime::UNIX_EPOCH
}

pub fn is_origin_of_time(value: &OffsetDateTime) -> bool {
    *value == origin_of_time()
}

pub fn is_zero(x: &usize) -> bool {
    *x == 0
}
