use btc_types::header::{ExtendedHeader, LightHeader};

pub trait BlocksGetter {
    fn get_prev_header(&self, current_header: &LightHeader) -> ExtendedHeader;
    #[allow(unused)]
    fn get_header_by_height(&self, height: u64) -> ExtendedHeader;
}
