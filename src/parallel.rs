use std::sync::Mutex;

/// Runs `work` over `items` on every available thread, returning the outputs
/// in unspecified order. A std-only work pool: jobs here are I/O-bound (file
/// reads, directory walks), so a mutex-based queue beats dep-heavy rayon.
/// The item set is fixed upfront — workers just drain the queue.
pub fn parallel_map<T, R>(items: Vec<T>, work: impl Fn(T) -> R + Sync) -> Vec<R>
where
    T: Send,
    R: Send,
{
    let threads = std::thread::available_parallelism()
        .map(|count| count.get())
        .unwrap_or(4)
        .min(items.len())
        .max(1);
    if threads <= 1 {
        return items.into_iter().map(work).collect();
    }

    let queue = Mutex::new(items);
    let results = Mutex::new(Vec::new());

    std::thread::scope(|scope| {
        for _ in 0..threads {
            scope.spawn(|| {
                while let Some(job) = queue.lock().unwrap().pop() {
                    let result = work(job);
                    results.lock().unwrap().push(result);
                }
            });
        }
    });

    results.into_inner().unwrap()
}

#[cfg(test)]
mod tests {
    use super::parallel_map;

    #[test]
    fn maps_every_item_exactly_once() {
        let mut outputs = parallel_map((0..1000).collect::<Vec<_>>(), |value| value * 2);
        outputs.sort_unstable();
        let expected: Vec<i32> = (0..1000).map(|value| value * 2).collect();
        assert_eq!(outputs, expected);
    }

    #[test]
    fn handles_a_single_job() {
        assert_eq!(parallel_map(vec![21], |value| value + 21), vec![42]);
    }

    #[test]
    fn handles_no_jobs() {
        let empty: Vec<i32> = vec![];
        assert_eq!(parallel_map(empty, |value| value + 1), Vec::<i32>::new());
    }
}
