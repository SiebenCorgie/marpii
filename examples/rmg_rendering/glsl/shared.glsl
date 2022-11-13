
struct SimObject{
    vec4 location;
    vec4 velocity;
};

struct ResHandle{
    uint hdl;
};

uint get_index(ResHandle res){
    return (res.hdl >> 8);
}

struct ForwardPush{
  ResHandle ubo;
  ResHandle sim;
  uvec2 pad;
};
