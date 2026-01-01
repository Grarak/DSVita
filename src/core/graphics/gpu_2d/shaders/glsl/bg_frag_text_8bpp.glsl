vec4 drawText(int x, int y, int bgNum) {
    int screenAddr = (((dispCnt >> 27) & 0x7) * 64 + ((bgCnt >> 8) & 0x1F) * 2) * 1024;
    int charAddr = (((dispCnt >> 24) & 0x7) * 64 + ((bgCnt >> 2) & 0xF) * 16) * 1024;

    int of = bgOfs[y * 4 + bgNum];
    x += of & 0xFFFF;
    x &= 0x1FF;
    y += of >> 16;
    y &= 0x1FF;

    // 512 Width
    if (x > 255 && (bgCnt & (1 << 14)) != 0) {
        screenAddr += 0x800;
    }

    // 512 Height
    if (y > 255 && (bgCnt & (1 << 15)) != 0) {
        screenAddr += (bgCnt & (1 << 14)) != 0 ? 0x1000 : 0x800;
    }

    int xBlock = x & 0xF8;
    int xInBlock = x & 7;
    int yBlock = y & 0xF8;
    int yInBlock = y & 7;

    screenAddr += yBlock << 3;
    screenAddr += xBlock >> 2;
    int screenEntry = readBg16Aligned(screenAddr);

    int isHFlip = (screenEntry >> 10) & 1;
    int isVFlip = (screenEntry >> 11) & 1;

    xInBlock = abs(isHFlip * 7 - xInBlock);
    yInBlock = abs(isVFlip * 7 - yInBlock);

    charAddr += ((screenEntry & 0x3FF) << 6) + (yInBlock << 3);
    charAddr += xInBlock;

    int palAddr = readBg8(charAddr);
    if (palAddr == 0) {
        discard;
    }
    palAddr *= 2;

    bool useExtPal = (dispCnt & (1 << 30)) != 0;
    if (useExtPal) {
        int slot = bgNum < 2 && (bgCnt & (1 << 13)) != 0 ? bgNum + 2 : bgNum;
        palAddr += slot * 8192 + ((screenEntry & 0xF000) >> 3);
        int color = readExtPal16Aligned(palAddr);
        return vec4(normRgb5(color), 1.0);
    } else {
        int color = readPal16Aligned(palAddr);
        return vec4(normRgb5(color), 1.0);
    }
}

void main() {
    int bgNum = int(screenPos.z);

    int winEnabled = int(texture(winTex, screenPosF).x * 255.0);
    if ((winEnabled & (1 << bgNum)) == 0) {
        discard;
    }

    int x = int(screenPos.x);
    int y = int(screenPos.y);

    color = drawText(x, y, bgNum);
    setPrio();
}
