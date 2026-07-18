in vec2 screenPosWidescreen;

bool bgMain() {
    color = texture(display3dTex, screenPosWidescreen);
    int winEnabled = int(texture(winTex, screenPosF).x);
    if ((winEnabled & 1) == 0) {
        discard;
    }

    if (color.a == 0.0) {
        discard;
    }

    int bgCnt = getBgCnt();
    int priority = bgCnt & 3;
    int alpha = int(color.a * 31.0);
    uint data = uint(priority) | uint(alpha << 2);
    fragOutColor.a = data;
    return false;
}
