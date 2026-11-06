//! A little randomness (for Random radio) without another dependency.

use std::cell::Cell;
use std::time::{SystemTime, UNIX_EPOCH};

thread_local! {
    static STATE: Cell<u64> = Cell::new(
        SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos() as u64).unwrap_or(0x9e37_79b9_7f4a_7c15) | 1,
    );
}

/// A number below `limit` (xorshift; plenty for picking songs).
pub fn below(limit: usize) -> usize {
    if limit == 0 {
        return 0;
    }
    STATE.with(|state| {
        let mut x = state.get();
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        state.set(x);
        (x % limit as u64) as usize
    })
}

/// The items in a random order.
pub fn shuffle<T>(items: &mut [T]) {
    for i in (1..items.len()).rev() {
        items.swap(i, below(i + 1));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Random radio from each service: `cargo test -- --ignored`.
    #[test]
    #[ignore]
    fn live_radio_from_every_service() {
        let sc = crate::soundcloud::related(501316227).unwrap();
        assert!(!sc.is_empty(), "SoundCloud");
        let mc = crate::mixcloud::similar("/snp-djmitsuki/dragon-quest-beats-iiiiii-megamix-mixed-by-dj-mitsuki/").unwrap();
        assert!(!mc.is_empty(), "Mixcloud");
        let album = crate::bandcamp::album("bc:https://mauricemori.bandcamp.com/album/dragon-quest-guitar-collection-1").unwrap();
        let bc = crate::bandcamp::related(&album["tracks"][0]).unwrap();
        assert!(!bc.is_empty(), "Bandcamp");
        let show = crate::archive::album("ia:gd1977-05-08.111493.mtx.seamons.sbeok.flac16").unwrap();
        let ia = crate::archive::related(&show["tracks"][0]).unwrap();
        assert!(!ia.is_empty(), "Internet Archive");
        assert!(ia.iter().all(|t| !t["key"].as_str().unwrap().contains("gd1977-05-08.111493")));
    }

    #[test]
    fn shuffles_everything() {
        let mut items: Vec<u32> = (0..50).collect();
        shuffle(&mut items);
        let mut sorted = items.clone();
        sorted.sort();
        assert_eq!(sorted, (0..50).collect::<Vec<_>>());
        assert!(below(1) == 0 && below(10) < 10);
    }
}
