#include <vitasdk.h>

int sceRazorCpuPushMarkerWithHud(const char* label, int color, int flags);
int sceRazorCpuPopMarker();

void __cyg_profile_func_enter(void *this_fn, void *call_site) {
    char addr[32];
    sprintf(addr, "func %x", this_fn);
    sceRazorCpuPushMarkerWithHud(addr, 0x8000ffff, 0);
}

void __cyg_profile_func_exit(void *this_fn, void *call_site) {
	sceRazorCpuPopMarker();
}

void _mcount() {
	sceRazorCpuPopMarker();
}
