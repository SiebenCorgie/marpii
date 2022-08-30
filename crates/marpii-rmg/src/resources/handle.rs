use std::marker::PhantomData;



///Device handle that can be used
pub struct DeviceHandle<T>{
    ty: PhantomData<T>,
}
