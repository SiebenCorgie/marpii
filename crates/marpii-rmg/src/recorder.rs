

pub(crate) mod task;

use thiserror::Error;

use crate::Rmg;

#[derive(Debug, Error)]
pub enum RecordError{

}


///records a new execution graph blocks any access to `rmg` until the graph is executed.
pub struct Recorder<'rmg>{
    pub rmg: &'rmg mut Rmg
}
