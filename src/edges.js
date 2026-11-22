// Trazos de contraste de la vista previa. La máscara definitiva la produce ONNX.
function contours(pixels, w, h) {
  const gray = new Float32Array(w * h);
  const magnitude = new Float32Array(w * h);
  for (let i = 0; i < gray.length; i++) {
    const k = i * 4;
    gray[i] = (.2126 * pixels[k] + .7152 * pixels[k+1] + .0722 * pixels[k+2]) * pixels[k+3] / 255;
  }
  let sum = 0;
  for (let y = 1; y < h-1; y++) {
    for (let x = 1; x < w-1; x++) {
      const i = y*w+x;
      const dx = gray[i-w+1] + 2*gray[i+1] + gray[i+w+1] - gray[i-w-1] - 2*gray[i-1] - gray[i+w-1];
      const dy = gray[i+w-1] + 2*gray[i+w] + gray[i+w+1] - gray[i-w-1] - 2*gray[i-w] - gray[i-w+1];
      magnitude[i] = Math.hypot(dx,dy);
      sum += magnitude[i];
    }
  }
  const threshold = Math.max(22, sum / gray.length * 1.8);
  const neighbors = [1,w,w+1,w-1,-1,-w,-w+1,-w-1];
  const paths = [];
  for (let i = w+1; i < magnitude.length-w; i++) {
    if (magnitude[i] < threshold) continue;
    const points = [];
    let current = i;
    while (magnitude[current] >= threshold) {
      points.push(current);
      magnitude[current] = 0;
      let best = current;
      for (const offset of neighbors) {
        const next = current + offset;
        if (magnitude[next] > magnitude[best]) best = next;
      }
      current = best;
    }
    if (points.length >= 6) paths.push(points);
  }
  return paths.sort((a,b) => b.length-a.length).slice(0,220);
}

export function animateEdges(canvas, image) {
  const scale = Math.min(1, 480 / Math.max(image.naturalWidth,image.naturalHeight));
  const w = canvas.width = Math.max(1, Math.round(image.naturalWidth*scale));
  const h = canvas.height = Math.max(1, Math.round(image.naturalHeight*scale));
  const ctx = canvas.getContext("2d");
  ctx.drawImage(image,0,0,w,h);
  const paths = contours(ctx.getImageData(0,0,w,h).data,w,h);
  const still = matchMedia("(prefers-reduced-motion: reduce)").matches;
  const start = performance.now();
  let frame, last = -100;
  function draw(time) {
    const t = (time-start)/1000;
    if (time-last >= 48 || still) {
      last = time;
      ctx.clearRect(0,0,w,h);
      ctx.drawImage(image,0,0,w,h);
      ctx.fillStyle = "rgba(4, 22, 24, .66)";
      ctx.fillRect(0,0,w,h);
      ctx.lineJoin = ctx.lineCap = "round";
      paths.forEach((points,j) => {
        const fraction = still ? 1 : Math.max(0,Math.min(1,(t-j*.003)*.75));
        const end = Math.floor(fraction*(points.length-1));
        if (end < 1) return;
        ctx.beginPath();
        ctx.moveTo(points[0]%w,Math.floor(points[0]/w));
        for (let n=1; n<=end; n++) ctx.lineTo(points[n]%w,Math.floor(points[n]/w));
        ctx.strokeStyle = j%5 === 0 ? "rgba(255,173,104,.92)" : "rgba(127,255,218,.85)";
        ctx.lineWidth = j<24 ? 1.05 : .55;
        ctx.stroke();
        if (!still && j < 42) {
          const tip = points[fraction < 1 ? end : Math.floor(((t*.20+j*.17)%1)*end)];
          ctx.fillStyle = "#fff4cf";
          ctx.fillRect(tip%w-1,Math.floor(tip/w)-1,2.2,2.2);
        }
      });
    }
    if (!still) frame=requestAnimationFrame(draw);
  }
  canvas.hidden=false;
  draw(start);
  return () => {
    cancelAnimationFrame(frame);
    canvas.hidden=true;
    canvas.width=canvas.height=1;
  };
}
