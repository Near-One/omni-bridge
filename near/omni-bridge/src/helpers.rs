use near_sdk::{
    env::{self, panic_str},
    serde_json, Promise, PromiseIndex,
};
use serde::Serialize;

pub trait SdkExpect<T> {
    fn sdk_expect(self, msg: &str) -> T;
}

impl<T> SdkExpect<T> for Option<T> {
    fn sdk_expect(self, msg: &str) -> T {
        self.unwrap_or_else(|| panic_str(msg))
    }
}

impl<T, E> SdkExpect<T> for Result<T, E> {
    fn sdk_expect(self, msg: &str) -> T {
        self.unwrap_or_else(|_| panic_str(msg))
    }
}

pub enum PromiseOrPromiseIndexOrValue<T> {
    Promise(Promise),
    PromiseIndex(PromiseIndex),
    Value(T),
}

impl<T> PromiseOrPromiseIndexOrValue<T>
where
    T: Serialize,
{
    pub fn as_return(self) {
        match self {
            PromiseOrPromiseIndexOrValue::Promise(promise) => {
                promise.as_return();
            }
            PromiseOrPromiseIndexOrValue::PromiseIndex(promise_index) => {
                env::promise_return(promise_index)
            }
            PromiseOrPromiseIndexOrValue::Value(value) => {
                env::value_return(&serde_json::to_vec(&value).unwrap())
            }
        }
    }
}

impl<T> From<Promise> for PromiseOrPromiseIndexOrValue<T> {
    fn from(promise: Promise) -> Self {
        PromiseOrPromiseIndexOrValue::Promise(promise)
    }
}
