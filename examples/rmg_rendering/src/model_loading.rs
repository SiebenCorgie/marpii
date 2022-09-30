use shared::Vertex;


pub fn load_model() -> (Vec<Vertex>, Vec<u32>){
    //currently generates a laying quad
    let vertices = vec![
        Vertex{position: [-1.0, 0.0, -1.0], normal: [0.0, 1.0, 0.0], uv: [0.0, 0.0]},
        Vertex{position: [ 1.0, 0.0, -1.0], normal: [0.0, 1.0, 0.0], uv: [1.0, 0.0]},
        Vertex{position: [-1.0, 0.0,  1.0], normal: [0.0, 1.0, 0.0], uv: [0.0, 1.0]},
        Vertex{position: [ 1.0, 0.0,  1.0], normal: [0.0, 1.0, 0.0], uv: [1.0, 1.0]},
    ];

    let indices = vec![
        0,1,2,
        2,1,3
    ];


    (vertices, indices)
}
