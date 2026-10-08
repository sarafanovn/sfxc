//! Order of reorderable cards: moving an item and restoring a saved order.

/// Moves `from` to position `to` (clamped to the last index). A bad `from` or the same position changes nothing.
pub fn move_item<T>(items: &mut Vec<T>, from: usize, to: usize) {
    if from >= items.len() {
        return;
    }
    let to = to.min(items.len() - 1);
    if from != to {
        let item = items.remove(from);
        items.insert(to, item);
    }
}

/// Order from its saved text: unknown and repeated keys are dropped, missing ones are appended in default order.
pub fn restore(saved: &str, defaults: &[&str]) -> Vec<String> {
    let mut out: Vec<String> = Vec::with_capacity(defaults.len());
    for key in saved.split(',').map(str::trim) {
        if defaults.contains(&key) && !out.iter().any(|k| k == key) {
            out.push(key.to_string());
        }
    }
    for key in defaults {
        if !out.iter().any(|k| k == key) {
            out.push((*key).to_string());
        }
    }
    out
}

pub fn save(order: &[String]) -> String {
    order.join(",")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn move_item_forward_and_back() {
        let mut v = vec![1, 2, 3, 4];
        move_item(&mut v, 0, 2);
        assert_eq!(v, vec![2, 3, 1, 4]);
        move_item(&mut v, 3, 0);
        assert_eq!(v, vec![4, 2, 3, 1]);
    }

    #[test]
    fn move_item_in_place_or_out_of_range_changes_nothing_or_clamps() {
        let mut v = vec![1, 2, 3];
        move_item(&mut v, 1, 1);
        move_item(&mut v, 5, 0);
        assert_eq!(v, vec![1, 2, 3]);
        move_item(&mut v, 0, 9);
        assert_eq!(v, vec![2, 3, 1]);
        let mut empty: Vec<i32> = Vec::new();
        move_item(&mut empty, 0, 0);
        assert!(empty.is_empty());
    }

    #[test]
    fn restore_keeps_saved_order_and_appends_new_keys() {
        let d = ["a", "b", "c", "d"];
        assert_eq!(restore("c,a", &d), vec!["c", "a", "b", "d"]);
    }

    #[test]
    fn restore_drops_unknown_and_duplicate_keys_and_survives_garbage() {
        let d = ["a", "b", "c"];
        assert_eq!(restore("b,zzz,b,a", &d), vec!["b", "a", "c"]);
        assert_eq!(restore("", &d), vec!["a", "b", "c"]);
        assert_eq!(restore(",,\u{0}, ,", &d), vec!["a", "b", "c"]);
    }

    #[test]
    fn save_round_trips() {
        let d = ["a", "b", "c"];
        let order = vec!["c".to_string(), "a".to_string(), "b".to_string()];
        assert_eq!(restore(&save(&order), &d), order);
    }
}
