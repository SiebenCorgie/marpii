


///Target pseudo code for recording a frame
fn record_frame(&mut self){
    self.graph.new_frame() //define new frame graph
        .graphics_pass(self.shadow_pass, [Access::Write("ShadowCascades")]) //add hock from pass handle that writes to one image
        .graphics_pass_with_prepare(  //defines known pass
            self.forward_pass,
            [Access::Read("ShadowCascades"), Access::Write("ForwardOpaque")]
            |fwp| fwp.set_meshes(self.meshes.clone())
        ) //allows change before execute
        .new_graphics("/path/to/shader.spv")
        .render(); //executes whenever possible
}

///Async task definition for anything none frame related like up/download and physics
fn task(&mut self){
    //setting up a new vertexbuffer
    let mesh_buffer: BufferHdl<Vertex> = self.graph.new_device_buffer(import_mesh("my/mesh.gltf"));
    self.meshes.push(mesh_buffer);

    //loading a image
    let texhdl: ImageHdl = self.graph.new_img_2d("texture.png");

    //loading a texture,
    let texhdl: ImageHdl = self.graph.new_tex_2d("texture.png");

    //loading a texture
    let texhdl: ImageHdl = self.graph.new_tex_2d_builder("texture.png") //allow changing settings
        .with_sampler(|sampler| sampler.repeate()); //for instance the sampler configuration



}
