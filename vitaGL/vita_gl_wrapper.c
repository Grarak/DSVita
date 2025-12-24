#include <shared.h>

void *vglRemapTexPtr() {
    texture_unit *tex_unit = &texture_units[server_texture_unit];
	int texture2d_idx = tex_unit->tex_id[0];
	texture *tex = &texture_slots[texture2d_idx];

	SceGxmTextureFormat tex_format = sceGxmTextureGetFormat(&tex->gxm_tex);
	uint8_t bpp = tex_format_to_bytespp(tex_format);
	uint32_t orig_w = sceGxmTextureGetWidth(&tex->gxm_tex);
	uint32_t orig_h = sceGxmTextureGetHeight(&tex->gxm_tex);
	uint32_t stride = VGL_ALIGN(orig_w, 8) * bpp;

	if (tex->last_frame != OBJ_NOT_USED && (vgl_framecount - tex->last_frame <= FRAME_PURGE_FREQ)) {
		void *texture_data = gpu_alloc_mapped(orig_h * stride, VGL_MEM_MAIN);
		gpu_free_texture_data(tex);
		sceGxmTextureSetData(&tex->gxm_tex, texture_data);
		tex->data = texture_data;
		tex->last_frame = OBJ_NOT_USED;
	}
	return tex->data;
}

void glTexImage2Drgba5(GLsizei width, GLsizei height) {
	// Setting some aliases to make code more readable
	texture_unit *tex_unit = &texture_units[server_texture_unit];
	int texture2d_idx = tex_unit->tex_id[0];
	texture *tex = &texture_slots[texture2d_idx];

	gpu_alloc_texture(width, height, SCE_GXM_TEXTURE_FORMAT_U1U5U5U5_ABGR, NULL, tex, 2, NULL, NULL, GL_TRUE);
}
