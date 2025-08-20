use btc_types::header::{ExtendedHeader, LightHeader};

pub trait BlocksGetter {
    fn get_prev_header(&self, current_header: &LightHeader) -> ExtendedHeader;
    #[allow(unused)]
    fn get_header_by_height(&self, height: u64) -> ExtendedHeader;
}

#[allow(unused)]
pub fn get_median_time_past(
    block_header: ExtendedHeader,
    prev_block_getter: &impl BlocksGetter,
) -> u32 {
    use btc_types::network::MEDIAN_TIME_SPAN;

    let mut median_time = [0u32; MEDIAN_TIME_SPAN];
    let mut current_header = block_header;

    for i in 0..MEDIAN_TIME_SPAN {
        median_time[i] = current_header.block_header.time;
        current_header = prev_block_getter.get_prev_header(&current_header.block_header);
    }

    median_time.sort_unstable();
    median_time[median_time.len() / 2]
}
