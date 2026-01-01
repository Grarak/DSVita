void main() {
    int winEnabled = int(texture(winTex, screenPosF).x * 255.0);
    if ((winEnabled & 1) == 0) {
        discard;
    }

    color = texture(display3dTex, vec2(screenPosF.x, 1.0 - screenPosF.y));

    if (color.a == 0.0) {
        discard;
    }

    int priority = bgCnt & 3;
    int alpha = int(color.a * 31.0);
    int data = priority | (alpha << 2);
    color.a = float(data) / 255.0;
}
