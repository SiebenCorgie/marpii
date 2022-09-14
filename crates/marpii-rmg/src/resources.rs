

pub(crate) mod descriptor;
pub(crate) mod res_states;

use thiserror::Error;

use self::descriptor::Bindless;


#[derive(Debug, Error)]
pub enum ResourceError{

}




pub struct Resources{
    bindless: Bindless,


}
