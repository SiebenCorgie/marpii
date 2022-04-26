use std::sync::Arc;

use marpii::sync::Semaphore;
use marpii_commands::Recorder;

use super::{AssumedState, Pass};

///Pass that blocks until `signal` is set.
pub struct WaitExternal {
    signal: Arc<Semaphore>,
}

impl WaitExternal {
    pub fn new(signal: Arc<Semaphore>) -> Self {
        WaitExternal { signal }
    }
}

impl Pass for WaitExternal {
    fn assumed_states(&self) -> &[AssumedState] {
        &[]
    }

    ///The actual recording step. Gets provided with access to the actual resources through the
    /// `ResourceManager` as well as the `command_buffer` recorder that is currently in use.
    fn record(&mut self, _command_buffer: &mut Recorder) -> Result<(), anyhow::Error> {
        Ok(()) //doesn't do anything.
    }

    fn waits_for_external(&self) -> &[Arc<Semaphore>] {
        core::slice::from_ref(&self.signal)
    }
}
