// 스크린샷 인식 패널: 붙여넣기/드롭 → 격자 검출 → 판·손패·아이콘 반영.
import { app } from '../app';
import { normCells } from '../presses';
import { emptySlot, store } from '../store';
import type { Cell, GameState } from '../types';
import { H, W } from '../types';
import { MAX_IMAGE_SIDE, recognize, recognizeWithRect, type Recognition, type Rect } from '../vision';
import { esc } from './mini';

interface Shot {
  canvas: HTMLCanvasElement;
  img: ImageData;
  w: number;
  h: number;
  rec: Recognition | null;
  picking: Array<[number, number]> | null;
  info: ShotInfo | null;
}

interface ShotInfo {
  err?: string;
  filled?: number;
  icE?: number;
  icR?: number;
  names?: string[];
  unknown?: number;
  followed?: number;
  inferred?: number;
  /** 새 손패 추정 시, 같은 조각이 같은 슬롯에 다시 나왔다고 본 슬롯 수. */
  assumedRepeat?: number;
  usedCal?: boolean;
}

let shot: Shot | null = null;

async function loadBitmap(file: Blob): Promise<ImageBitmap | HTMLImageElement> {
  if ('createImageBitmap' in window) return createImageBitmap(file);
  return new Promise((res, rej) => {
    const u = URL.createObjectURL(file);
    const im = new Image();
    im.onload = () => res(im);
    im.onerror = rej;
    im.src = u;
  });
}

export async function handleImage(file: Blob): Promise<void> {
  try {
    const bmp = await loadBitmap(file);
    const iw = (bmp as HTMLImageElement).naturalWidth || bmp.width;
    const ih = (bmp as HTMLImageElement).naturalHeight || bmp.height;
    const sc = Math.min(1, MAX_IMAGE_SIDE / Math.max(iw, ih));
    const cw = Math.max(1, Math.round(iw * sc));
    const ch = Math.max(1, Math.round(ih * sc));
    const cv = document.createElement('canvas');
    cv.width = cw;
    cv.height = ch;
    const cx = cv.getContext('2d', { willReadFrequently: true })!;
    cx.drawImage(bmp, 0, 0, cw, ch);
    const img = cx.getImageData(0, 0, cw, ch);
    const cal = store.state.cal;
    const rec = recognize(img, W, H, cal && cal.iw === cw && cal.ih === ch ? cal : null);
    shot = { canvas: cv, img, w: cw, h: ch, rec, picking: null, info: null };
    document.getElementById('shotResult')!.hidden = false;
    if (!rec.ok) {
      shot.info = { err: '게임판 격자를 찾지 못했습니다. 「판 위치 직접 지정」을 누르고 판의 왼쪽 위 모서리와 오른쪽 아래 모서리를 차례로 클릭하세요.' };
      renderShot();
      return;
    }
    applyShot(rec);
  } catch (e) {
    document.getElementById('shotResult')!.hidden = false;
    shot = null;
    document.getElementById('shotMsg')!.innerHTML = `<div class="banner warn">이미지를 읽지 못했습니다 (${esc(String(e))}). PNG나 JPG 캡처를 붙여넣어 주세요.</div>`;
  }
}

function applyShot(rec: Recognition): void {
  if (!shot) return;
  store.snapshot();
  const prev = store.state.game;
  const board: string[] = [];
  const rows: string[][] = Array.from({ length: H }, () => Array.from({ length: W }, () => '.'));
  let filled = 0;
  let icE = 0;
  let icR = 0;
  const iconCells: Array<{ r: number; c: number; kind: 'Dot' | 'Reroll' }> = [];
  for (const o of rec.cells) {
    if (o.state === 'filled') {
      rows[o.r][o.c] = '#';
      filled++;
    }
    if (o.icon) {
      const kind = o.ab === 'r' ? 'Reroll' : 'Dot';
      iconCells.push({ r: o.r, c: o.c, kind });
      if (kind === 'Reroll') icR++;
      else icE++;
    }
  }
  for (let r = 0; r < H; r++) board.push(rows[r].join(''));

  // 추천대로 놓았는지: 프레임의 after 판과 같으면 그 단계까지 반영된 것으로 본다 (점수·배치 카운터가 맞는다).
  let followed = 0;
  let base: GameState = prev;
  for (let k = 0; k < app.frames.length; k++) {
    if (app.frames[k].after.board.join('') === board.join('')) {
      followed = k + 1;
      base = app.frames[k].after;
    }
  }
  const g: GameState = { ...base, board, icons: [] };
  // 아이콘: 같은 위치면 기존 seq 유지, 새 위치는 새 seq. 인식된 것만 남긴다.
  const keep = iconCells.map((ic) => {
    const old = base.icons.find((x) => x.r === ic.r && x.c === ic.c);
    return old ? { ...old, kind: ic.kind } : { ...ic, seq: g.icon_seq++ };
  });
  keep.sort((a, b) => a.seq - b.seq);
  g.icons = keep.slice(-3);

  // 손패
  const names: string[] = [];
  let unknown = 0;
  let inferred = 0;
  let assumedRepeat = 0;
  if (rec.hand.cards.length) {
    const newHand: (number | null)[] = [null, null, null];
    const newSlots = [emptySlot(), emptySlot(), emptySlot()];
    const oldHand = followed ? base.hand : prev.hand;
    const oldSlots = store.state.slots;
    for (let i = 0; i < 3; i++) {
      const cd = rec.hand.cards[i];
      if (!cd || !cd.cells) continue;
      const pid = app.matchPiece(cd.cells as Cell[]);
      if (pid === null) {
        unknown++;
        continue;
      }
      newHand[i] = pid;
      const same = oldHand[i] === pid;
      newSlots[i] = { cur: normCells(cd.cells as Cell[]), counted: same ? oldSlots[i].counted : false };
      names.push(app.piece(pid).name);
    }
    if (!followed) {
      // 추천을 따르지 않았다면 손패 변화로 배치 수를 추정한다.
      const act = [0, 1, 2].filter((j) => prev.hand[j] !== null);
      const emptied = act.filter((j) => newHand[j] === null);
      const changed = act.filter((j) => newHand[j] !== null && newHand[j] !== prev.hand[j]);
      // 비워진 슬롯 없이 하나라도 바뀌었으면 새 손패가 나온 것(= 남은 조각을 전부 놓음)으로 본다.
      // 앱의 바꿔 뽑기 흐름은 슬롯을 먼저 비우므로, '바뀌었지만 비워지지 않은' 슬롯은 바꿔 뽑기에서 나올 수 없다.
      const placedN = act.length && !emptied.length && changed.length > 0 ? act.length : emptied.length;
      if (placedN === act.length && changed.length < act.length) assumedRepeat = act.length - changed.length;
      g.placements += placedN;
      inferred = placedN;
    }
    for (let i = 0; i < 3; i++) {
      if (newHand[i] !== null && !newSlots[i].counted) {
        store.state.settings.counts[newHand[i]!]++;
        newSlots[i].counted = true;
      }
    }
    g.hand = newHand;
    store.state.slots = newSlots;
    const nx = newHand.findIndex((h) => h === null);
    store.state.target = nx >= 0 ? nx : 0;
  }
  g.over = false;
  app.setGame(g);
  store.state.example = false;
  store.state.cal = { iw: shot.w, ih: shot.h, x0: rec.rect.x0, y0: rec.rect.y0, p: rec.rect.p, q: rec.rect.q };
  shot.rec = rec;
  shot.info = { filled, icE, icR, names, unknown, followed, inferred, assumedRepeat, usedCal: rec.usedCal };
  app.invalidate();
  store.save();
  app.renderAll();
  renderShot();
  if (app.handCount() > 0) app.scheduleSolve();
}

export function renderShot(): void {
  if (!shot) return;
  const inf = shot.info ?? {};
  let msg = '';
  if (inf.err) msg = `<div class="banner warn">${esc(inf.err)}</div>`;
  else {
    msg = `<div class="banner ok">판 ${W}×${H} 인식 · 채운 칸 <b class="num">${inf.filled ?? 0}</b>개` +
      (inf.names && inf.names.length ? ` · 손패 <b>${inf.names.map(esc).join(', ')}</b>` : ' · 손패는 찾지 못해 그대로 두었습니다') +
      (inf.usedCal ? ' · 저장된 판 위치 사용' : '') + '</div>';
    if (inf.unknown) msg += `<div class="small warn-text">라이브러리에 없는 모양의 손패 카드가 ${inf.unknown}개 있습니다. 19종 외의 조각은 없다고 알고 있으니 인식 오류일 가능성이 큽니다. 슬롯에 직접 넣어주세요.</div>`;
    if (inf.followed) msg += `<div class="small muted">추천 ${inf.followed}단계까지 그대로 놓은 것으로 확인되어 점수·배치 횟수·능력을 반영했습니다.</div>`;
    if (inf.inferred) msg += `<div class="small muted">손패 변화로 보아 조각 ${inf.inferred}개를 놓은 것으로 보고 배치 횟수를 늘렸습니다.${inf.assumedRepeat ? ` 슬롯 ${inf.assumedRepeat}개는 같은 조각이 다시 나온 것으로 보았습니다.` : ''} 게임 숫자와 다르면 −/+로 맞춰주세요.</div>`;
    if (inf.icE || inf.icR) msg += `<div class="small">능력 아이콘을 찾았습니다: ${inf.icE ? `⊙ 점 찍기 ${inf.icE}개 ` : ''}${inf.icR ? `⇄ 바꿔 뽑기 ${inf.icR}개` : ''}. 종류가 틀리면 능력 아이콘 모드에서 그 칸을 눌러 바꾸세요.</div>`;
    msg += '<div class="small muted">빨간 격자와 점이 실제 판과 어긋나면 「판 위치 직접 지정」을 누르세요. 잘못 읽었으면 되돌리기로 이전 판으로 돌아갑니다.</div>';
  }
  if (shot.picking) msg = `<div class="banner soft">${shot.picking.length ? '이제 판의 <b>오른쪽 아래</b> 모서리를 클릭하세요.' : '이미지에서 판 격자의 <b>왼쪽 위</b> 모서리를 클릭하세요.'}</div>`;
  document.getElementById('shotMsg')!.innerHTML = msg;
  drawShot();
}

function drawShot(): void {
  if (!shot) return;
  const cv = document.getElementById('shotCanvas') as HTMLCanvasElement;
  const panel = document.getElementById('shotPanel')!;
  const avail = Math.max(200, panel.clientWidth - 8);
  const sc = Math.min(1.5, avail / shot.w);
  cv.width = Math.round(shot.w * sc);
  cv.height = Math.round(shot.h * sc);
  cv.classList.toggle('picking', !!shot.picking);
  const cx = cv.getContext('2d')!;
  cx.drawImage(shot.canvas, 0, 0, cv.width, cv.height);
  const rec = shot.rec;
  if (rec && rec.ok && !shot.picking) {
    const R = rec.rect;
    cx.strokeStyle = 'rgba(255,40,60,.9)';
    cx.lineWidth = 1;
    cx.beginPath();
    for (let k = 0; k <= W; k++) {
      const x = (R.x0 + k * R.p) * sc;
      cx.moveTo(x, R.y0 * sc);
      cx.lineTo(x, (R.y0 + H * R.q) * sc);
    }
    for (let k = 0; k <= H; k++) {
      const y = (R.y0 + k * R.q) * sc;
      cx.moveTo(R.x0 * sc, y);
      cx.lineTo((R.x0 + W * R.p) * sc, y);
    }
    cx.stroke();
    for (const o of rec.cells) {
      const x = (R.x0 + (o.c + 0.5) * R.p) * sc;
      const y = (R.y0 + (o.r + 0.5) * R.q) * sc;
      if (o.icon) {
        cx.beginPath();
        cx.arc(x, y, Math.max(4, R.p * sc * 0.42), 0, 7);
        cx.lineWidth = 2.5;
        cx.strokeStyle = o.ab === 'r' ? 'rgba(170,60,230,.95)' : 'rgba(255,200,0,.95)';
        cx.stroke();
      }
      if (o.state === 'filled') {
        cx.beginPath();
        cx.arc(x, y, Math.max(2, R.p * sc * 0.16), 0, 7);
        cx.fillStyle = 'rgba(20,20,30,.85)';
        cx.fill();
      }
    }
    rec.hand.cards.forEach((cd, i) => {
      cx.strokeStyle = 'rgba(255,140,0,.95)';
      cx.lineWidth = 2;
      if (cd.pb) cx.strokeRect((cd.pb.x0 - 2) * sc, (cd.pb.y0 - 2) * sc, (cd.pb.bw + 4) * sc, (cd.pb.bh + 4) * sc);
      cx.fillStyle = 'rgba(255,140,0,.95)';
      cx.font = `bold ${Math.max(11, Math.round(14 * sc))}px sans-serif`;
      cx.fillText(String(i + 1), (cd.x0 + 4) * sc, (cd.y0 + 16) * sc);
    });
  }
  if (shot.picking) {
    for (const p of shot.picking) {
      cx.beginPath();
      cx.arc(p[0] * sc, p[1] * sc, 5, 0, 7);
      cx.fillStyle = 'rgba(255,40,60,.95)';
      cx.fill();
    }
  }
}

export function initShot(): void {
  const cv = document.getElementById('shotCanvas') as HTMLCanvasElement;
  cv.addEventListener('click', (e) => {
    if (!shot || !shot.picking) return;
    const b = cv.getBoundingClientRect();
    const x = ((e.clientX - b.left) / b.width) * shot.w;
    const y = ((e.clientY - b.top) / b.height) * shot.h;
    shot.picking.push([x, y]);
    if (shot.picking.length === 2) {
      const [a, c] = shot.picking;
      shot.picking = null;
      if (c[0] - a[0] < W * 3 || c[1] - a[1] < H * 3) {
        shot.info = { err: '두 점이 너무 가깝습니다. 왼쪽 위, 오른쪽 아래 순서로 다시 클릭하세요.' };
        renderShot();
        return;
      }
      const rect: Rect = { x0: a[0], y0: a[1], p: (c[0] - a[0]) / W, q: (c[1] - a[1]) / H };
      const rec = recognizeWithRect(shot.img, W, H, rect);
      applyShot(rec);
      return;
    }
    renderShot();
  });
  document.getElementById('b-manual')!.onclick = () => {
    if (!shot) {
      app.status('먼저 스크린샷을 붙여넣으세요.', 'warn');
      return;
    }
    shot.picking = [];
    renderShot();
  };
  document.getElementById('b-resetCal')!.onclick = () => {
    store.state.cal = null;
    store.save();
    app.status('저장된 판 위치를 지웠습니다.', 'ok');
  };
  document.getElementById('b-shotHide')!.onclick = () => {
    document.getElementById('shotResult')!.hidden = true;
  };
  const fileInput = document.getElementById('shotFile') as HTMLInputElement;
  fileInput.onchange = () => {
    const f = fileInput.files?.[0];
    if (f) void handleImage(f);
    fileInput.value = '';
  };
  document.addEventListener('paste', (e) => {
    const items = e.clipboardData?.items ?? ([] as unknown as DataTransferItemList);
    for (let i = 0; i < items.length; i++) {
      const it = items[i];
      if (it.type && it.type.startsWith('image/')) {
        const f = it.getAsFile();
        if (f) {
          e.preventDefault();
          void handleImage(f);
          return;
        }
      }
    }
  });
  const drop = document.getElementById('drop')!;
  document.addEventListener('dragover', (e) => {
    if (e.dataTransfer && Array.from(e.dataTransfer.types).includes('Files')) {
      e.preventDefault();
      drop.classList.add('over');
    }
  });
  document.addEventListener('dragleave', (e) => {
    if (!e.relatedTarget) drop.classList.remove('over');
  });
  document.addEventListener('drop', (e) => {
    const f = e.dataTransfer?.files?.[0];
    drop.classList.remove('over');
    if (f && f.type.startsWith('image/')) {
      e.preventDefault();
      void handleImage(f);
    }
  });
  window.addEventListener('resize', () => {
    if (shot && !document.getElementById('shotResult')!.hidden) drawShot();
  });
}
