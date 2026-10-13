//! Bounded desktop preparation of independent imported bodies, collected in file order.
use super::{Body, Result, guard, mt};

pub(super) struct Job {
    pub solid: mt::Solid,
    pub check_mesh: bool,
}

type Prepared = Result<(Body, usize)>;

pub(super) fn workers() -> usize {
    #[cfg(not(target_arch = "wasm32"))]
    {
        std::thread::available_parallelism().map_or(1, |n| n.get().min(8))
    }
    #[cfg(target_arch = "wasm32")]
    {
        1
    }
}

fn prepare(job: &Job, placement: &mt::Matrix4) -> Prepared {
    guard("STEP body preparation", || {
        let solid = if *placement == mt::Matrix4::from_scale(1.0) { job.solid.clone() } else { mt::builder::transformed(&job.solid, *placement) };
        let body = Body::new(solid)?;
        let missing = if job.check_mesh { body.unmeshed_faces() } else { 0 };
        Ok((body, missing))
    })
}

pub(super) fn bodies(solids: &[Job], placement: &mt::Matrix4, workers: usize) -> Vec<Prepared> {
    #[cfg(not(target_arch = "wasm32"))]
    if solids.len() >= 4 && workers > 1 {
        return std::thread::scope(|scope| {
            let size = solids.len().div_ceil(workers.min(8));
            let mut pending = Vec::new();
            for chunk in solids.chunks(size) {
                let handle = std::thread::Builder::new()
                    .name("step-body".into())
                    .spawn_scoped(scope, move || chunk.iter().map(|solid| prepare(solid, placement)).collect::<Vec<_>>())
                    .ok();
                pending.push((chunk, handle));
            }
            let mut output = Vec::with_capacity(solids.len());
            for (chunk, handle) in pending {
                match handle {
                    Some(handle) => match handle.join() {
                        Ok(results) => output.extend(results),
                        Err(_) => output.extend(chunk.iter().map(|_| Err(crate::KernelError::Internal("STEP body worker panicked".into())))),
                    },
                    // Resource exhaustion must not lose an import: use the serial path.
                    None => output.extend(chunk.iter().map(|solid| prepare(solid, placement))),
                }
            }
            output
        });
    }
    let _ = workers;
    solids.iter().map(|solid| prepare(solid, placement)).collect()
}
