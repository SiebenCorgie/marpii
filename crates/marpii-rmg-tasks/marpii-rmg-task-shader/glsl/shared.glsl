
#define TYPE_STORAGE_BUFFER 0x0
#define TYPE_STORAGE_IMAGE 0x1
#define TYPE_SAMPLED_IMAGE 0x2
#define TYPE_SAMPLER 0x3
#define TYPE_ACCELERATION_STRUCTURE 0x4
#define TYPE_INVALID 0xff

struct ResHandle{
    uint hdl;
};

uint get_index(ResHandle res){
    return (res.hdl >> 8);
}

uint get_type(ResHandle res){
    return res.hdl & 0x000000ff;
}

bool is_valid(ResHandle res){
    return true;
}
