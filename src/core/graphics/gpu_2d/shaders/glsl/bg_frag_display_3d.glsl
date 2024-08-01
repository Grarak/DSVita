void main() {
    int bgNum = int(screenPos.z);

    int winEnabled = int(texture(winTex, screenPosF).x * 255.0);
    if ((winEnabled & (1 << bgNum)) == 0) {
        discard;
    }

    color = texture(display3dTex, vec2(screenPosF.x, 1.0 - screenPosF.y));
    if (color.a == 0.0) {
        discard;
    }

    int priority = bgCnt & 3;
    color.a = float(priority) / 4.0;
}
