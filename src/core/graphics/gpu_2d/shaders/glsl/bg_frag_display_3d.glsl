in vec2 screenPosWidescreen;

void main() {
    color = texture(display3dTex, screenPosWidescreen);
    int winEnabled = int(texture(winTex, screenPosF).x * 255.0);
    if ((winEnabled & 1) == 0) {
        discard;
    }

    if (color.a == 0.0) {
        discard;
    }

    int bgCnt = getBgCnt();
    int priority = bgCnt & 3;
    int alpha = int(color.a * 31.0);
    int data = priority | (alpha << 2);
    color.a = float(data) / 255.0;
}
