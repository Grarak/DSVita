import struct,sys
def xwd2png(path,out):
    d=open(path,'rb').read();hsz=struct.unpack('>I',d[0:4])[0];f=struct.unpack('>25I',d[4:104])
    w,h=f[3],f[4];bpp=f[10];bpl=f[11];rm,gm,bm=f[13],f[14],f[15];ncol=f[18]
    off=hsz+ncol*12;pix=d[off:]
    from PIL import Image;img=Image.new('RGB',(w,h));px=img.load();Bpp=bpp//8
    def sh(m):
        s=0
        while m and not(m&1):m>>=1;s+=1
        return s
    rs,gs,bs=sh(rm),sh(gm),sh(bm)
    for y in range(h):
        row=pix[y*bpl:(y+1)*bpl]
        for x in range(w):
            o=x*Bpp
            v=struct.unpack('<I',row[o:o+4])[0] if Bpp==4 else (row[o]|row[o+1]<<8|row[o+2]<<16)
            px[x,y]=(((v&rm)>>rs)&255,((v&gm)>>gs)&255,((v&bm)>>bs)&255)
    img.save(out);print("saved",out,w,h)
xwd2png(sys.argv[1],sys.argv[2])
