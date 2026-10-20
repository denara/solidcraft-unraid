//! Bounded desktop preparation of independent imported bodies, collected in file order.
use std::sync::Arc;

use super::{Body, Ctx, Exchange, KernelError, Units, brep, guard, mt};

pub(super) struct Job {
    pub source: Source,
    /// Where the body goes (its instance's placement in the assembly).
    pub placement: mt::Matrix4,
    pub check_mesh: bool,
}

/// What a job places: a solid item to read from the file (and orient), or a part an earlier
/// instance of the same item already read and oriented.
pub(super) enum Source {
    Item { item: u64, units: Units },
    Oriented(Arc<Part>),
}

/// A solid item read and oriented where the file defines it, placed by each of its instances.
pub(super) struct Part {
    pub body: Body,
    /// The shells are closed.
    pub closed: bool,
    /// Faces the file had.
    pub file_faces: usize,
    /// The file face of each kernel face.
    pub face_origin: Vec<u64>,
}

/// A prepared instance.
pub(super) struct Made {
    /// The part, placed.
    pub body: Body,
    /// Faces the mesher missed (counted when the job asked).
    pub missing: usize,
    pub part: Arc<Part>,
    /// This job read the item: the faces the read left out, with the reason.
    pub read: Option<Vec<String>>,
}

/// Why a job made no body: the item could not be read (`read`), or its body not made.
pub(super) struct Failure {
    pub read: bool,
    pub msg: String,
}

type Prepared = std::result::Result<Made, Failure>;

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

fn prepare(ex: &Exchange, job: &Job) -> Prepared {
    let (part, read) = match &job.source {
        Source::Oriented(part) => (part.clone(), None),
        Source::Item { item, units } => {
            let cx = Ctx { ex, len: units.len, ang: units.ang, tol: units.tol };
            let out = guard("step brep", || brep::read_item(&cx, *item).map_err(KernelError::Failed))
                .map_err(|e| Failure { read: true, msg: e.to_string().trim_start_matches("the operation failed: ").to_string() })?;
            // Orient once where the file defines the solid; each placement then takes its
            // orientation from the matrix instead of meshing the whole part again.
            let body = guard("STEP body preparation", || Body::new(out.solid)).map_err(|e| Failure { read: false, msg: e.to_string() })?;
            (Arc::new(Part { body, closed: out.closed, file_faces: out.file_faces, face_origin: out.face_origin }), Some(out.failed))
        }
    };
    guard("STEP body preparation", || {
        let body = part.body.placed(&job.placement)?;
        // Counted where the orientation mesh already counted them (a placement meshes the
        // same faces).
        let missing = if job.check_mesh { part.body.unmeshed_faces() } else { 0 };
        Ok((body, missing))
    })
    .map(|(body, missing)| Made { body, missing, part, read })
    .map_err(|e| Failure { read: false, msg: e.to_string() })
}

/// Prepare `jobs`, the results in the jobs' order. Solids still to read and orient (the slow
/// part) are shared out between up to `workers` threads, each taking the next job when it is
/// done with one, so one large part does not hold up the rest.
pub(super) fn bodies(ex: &Exchange, jobs: &[Job], workers: usize) -> Vec<Prepared> {
    let to_orient = jobs.iter().filter(|j| matches!(j.source, Source::Item { .. })).count();
    if to_orient > 1 && workers > 1 {
        return crate::par::map_on_threads(jobs, workers.min(8).min(to_orient), |job| prepare(ex, job))
            .into_iter()
            .map(|r| r.unwrap_or_else(|| Err(Failure { read: false, msg: "STEP body worker panicked".into() })))
            .collect();
    }
    jobs.iter().map(|job| prepare(ex, job)).collect()
}
