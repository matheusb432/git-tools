use gtl_models::failure::ErrorMeta;

#[derive(Debug, ErrorMeta)]
enum Missing {
    Unclassified,
    AlsoUnclassified(u8),
}

fn main() {}
