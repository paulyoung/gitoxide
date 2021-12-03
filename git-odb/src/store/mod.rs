pub use sink::{sink, Sink};

pub mod compound;
pub mod handle;
pub mod linked;
pub mod loose;

///
pub mod sink;

use git_features::threading::OwnShared;

/// Note that this type is only `Send` if `git-features/parallel` is toggled on.
pub struct Handle<S> {
    store: S,
    new_pack_cache: Option<OwnShared<handle::NewPackCacheFn>>,
    new_object_cache: Option<OwnShared<handle::NewObjectCacheFn>>,
    pack_cache: Option<Box<handle::PackCache>>,
    object_cache: Option<Box<handle::ObjectCache>>,
}
