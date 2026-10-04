import { useEffect, useRef, useState } from "react";
import jsQR from "jsqr";

interface Props {
  onScan: (payload: string) => void;
  onClose: () => void;
}

/**
 * 摄像头扫码组件：
 * 优先使用浏览器原生 BarcodeDetector（Android WebView / Chrome 支持），
 * 不可用时回退到 jsQR（video → canvas 逐帧解码）。
 */
export default function Scanner({ onScan, onClose }: Props) {
  const videoRef = useRef<HTMLVideoElement>(null);
  const [error, setError] = useState("");
  const stoppedRef = useRef(false);

  useEffect(() => {
    let stream: MediaStream | null = null;
    let raf = 0;
    let cv: HTMLCanvasElement | null = null;
    let ctx: CanvasRenderingContext2D | null = null;
    let detector: any = null;
    stoppedRef.current = false;

    const tryNative = async () => {
      try {
        const BarcodeDetector = (window as any).BarcodeDetector;
        if (BarcodeDetector && (await BarcodeDetector.getSupportedFormats?.()).includes("qr_code")) {
          detector = new BarcodeDetector({ formats: ["qr_code"] });
        }
      } catch {
        detector = null;
      }
    };

    const loop = async () => {
      if (stoppedRef.current) return;
      const video = videoRef.current;
      if (!video || video.readyState < 2) {
        raf = requestAnimationFrame(loop);
        return;
      }
      try {
        let payload: string | null = null;
        if (detector) {
          const codes = await detector.detect(video);
          if (codes.length > 0 && codes[0].rawValue) payload = codes[0].rawValue;
        } else {
          if (!cv) {
            cv = document.createElement("canvas");
            ctx = cv.getContext("2d", { willReadFrequently: true })!;
          }
          const w = video.videoWidth || 640;
          const h = video.videoHeight || 480;
          cv.width = w;
          cv.height = h;
          ctx!.drawImage(video, 0, 0, w, h);
          const img = ctx!.getImageData(0, 0, w, h);
          const code = jsQR(img.data, w, h, { inversionAttempts: "dontInvert" });
          if (code?.data) payload = code.data;
        }
        if (payload) {
          onScan(payload);
          return;
        }
      } catch {
        /* 单帧失败继续 */
      }
      raf = requestAnimationFrame(loop);
    };

    const start = async () => {
      try {
        await tryNative();
        stream = await navigator.mediaDevices.getUserMedia({
          video: { facingMode: "environment" },
          audio: false,
        });
        if (stoppedRef.current) {
          stream.getTracks().forEach((t) => t.stop());
          return;
        }
        if (videoRef.current) {
          videoRef.current.srcObject = stream;
          await videoRef.current.play();
        }
        raf = requestAnimationFrame(loop);
      } catch (e: any) {
        setError(
          e?.name === "NotAllowedError"
            ? "相机权限被拒绝，请在系统设置中允许 LocalVault 使用相机"
            : `无法启动相机：${e?.message || String(e)}`
        );
      }
    };

    start();

    return () => {
      stoppedRef.current = true;
      cancelAnimationFrame(raf);
      stream?.getTracks().forEach((t) => t.stop());
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  return (
    <div className="scanner-overlay">
      <div className="scanner-box">
        <video ref={videoRef} playsInline muted className="scanner-video" />
        <div className="scan-frame" />
        {error && <div className="error-box">{error}</div>}
        <p className="scanner-hint">将桌面端「手机同步」页面的二维码对准取景框</p>
        <button className="btn-secondary" onClick={onClose}>
          取消
        </button>
      </div>
    </div>
  );
}
