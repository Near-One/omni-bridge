use near_sdk::env::panic_str;

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
