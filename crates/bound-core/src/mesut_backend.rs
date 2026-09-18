//! Opt-in file loading through Mesut's Blocking and Compute executors.

use super::*;
use mesut::prelude::*;
use mesut::TaskError;
use std::sync::Arc;

fn runtime_error(error: impl std::fmt::Display) -> BundleError {
    BundleError::Io(std::io::Error::other(format!("Mesut: {error}")))
}

fn task_error(error: impl std::fmt::Display) -> TaskError {
    TaskError::ExecutionFailed(error.to_string())
}

/// Bundle using bounded read → metadata/hash DAGs, preserving snapshot order.
///
/// This synchronous entry point owns a Tokio runtime with timers. Async callers
/// should invoke it with `spawn_blocking`, as with the synchronous `bundle` API.
/// File read failures are skipped; runtime failures are returned as IO errors.
pub fn bundle_with_mesut(
    options: &BundleOptions,
    logger: &Logger,
) -> Result<BundleOutput, BundleError> {
    if tokio::runtime::Handle::try_current().is_ok() {
        return Err(runtime_error(
            "call bundle_with_mesut outside async context (use spawn_blocking)",
        ));
    }
    let driver = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()?;
    let runtime = MesuT::new(
        RuntimeConfig::new()
            .with_pipeline_concurrency(8)
            .with_animations(false),
    )
    .with_blocking_executor(Arc::new(BlockingExecutor::new(Default::default())))
    .with_compute_executor(Arc::new(
        RayonExecutor::new(Default::default()).map_err(runtime_error)?,
    ));
    let result = bundle_with_loader(options, logger, |paths, root, options| {
        let mut pipeline = Pipeline::new();
        let mut results = Vec::with_capacity(paths.len());
        for path in paths {
            let path = path.clone();
            let root = root.to_path_buf();
            let include_meta = options.include_meta;
            let hash = options.include_meta_hash;
            let read = PipelineStage::new("bound.read".into(), WorkKind::Blocking).with_work(
                Work::new(WorkKind::Blocking).with_job(move |_| {
                    let content = fs::read_to_string(&path).ok();
                    let meta = if include_meta && content.is_some() {
                        fs::metadata(&path).ok().map(|meta| FileMetadata {
                            relative_path: path
                                .strip_prefix(&root)
                                .unwrap_or(&path)
                                .to_string_lossy()
                                .into_owned(),
                            size_bytes: meta.len(),
                            modified_unix: meta
                                .modified()
                                .unwrap_or(UNIX_EPOCH)
                                .duration_since(UNIX_EPOCH)
                                .unwrap_or_default()
                                .as_secs(),
                            line_count: 0,
                            sha256: None,
                        })
                    } else {
                        None
                    };
                    serde_json::to_vec(&LoadedFile { meta, content }).map_err(task_error)
                }),
            );
            let read_id = read.id;
            let compute = PipelineStage::new("bound.metadata_hash".into(), WorkKind::Compute)
                .with_dependency(read_id)
                .with_factory(move |inputs| {
                    let input = inputs[&read_id].clone();
                    Ok(Work::new(WorkKind::Compute).with_job(move |_| {
                        let mut loaded: LoadedFile =
                            serde_json::from_slice(&input).map_err(task_error)?;
                        if let (Some(meta), Some(content)) = (&mut loaded.meta, &loaded.content) {
                            meta.line_count = content.lines().count();
                            meta.sha256 = hash.then(|| metadata::hash_string(content));
                        }
                        serde_json::to_vec(&loaded).map_err(task_error)
                    }))
                });
            results.push(compute.id);
            pipeline = pipeline.add_stage(read).add_stage(compute);
        }
        let outputs = driver
            .block_on(runtime.execute_pipeline(pipeline))
            .map_err(runtime_error)?;
        results
            .into_iter()
            .map(|id| serde_json::from_slice(&outputs[&id]).map_err(BundleError::from))
            .collect()
    });
    // Drain and close executors even if discovery, redaction, or a stage failed.
    let shutdown = driver.block_on(runtime.shutdown()).map_err(runtime_error);
    let output = result?;
    shutdown?;
    Ok(output)
}
