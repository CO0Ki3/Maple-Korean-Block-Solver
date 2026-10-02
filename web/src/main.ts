import './style.css';
import { app } from './app';
import { store } from './store';
import { initBoard, renderBoard } from './ui/board';
import { renderHand, renderLib } from './ui/hand';
import { initPlan, renderPlan } from './ui/plan';
import { renderAbilities } from './ui/abilities';
import { renderSettings } from './ui/settings';
import { initShot } from './ui/shot';
import { initSim, renderSim } from './ui/simview';

async function boot(): Promise<void> {
  app.status('엔진 불러오는 중…');
  await app.init();
  await loadTrainedWeightsIfDefault();
  app.onRender(renderBoard);
  app.onRender(renderAbilities);
  app.onRender(renderHand);
  app.onRender(renderLib);
  app.onRender(renderPlan);
  initBoard();
  initPlan();
  initShot();
  initSim();
  renderSettings();
  initTabs();
  document.getElementById('b-undo')!.onclick = () => {
    app.invalidate();
    if (!store.undo()) app.status('되돌릴 것이 없습니다.', 'warn');
    renderSettings(); // store.state 가 교체되므로 설정 패널의 핸들러도 새로 묶는다
    app.renderAll();
    app.scheduleSolve();
  };
  document.getElementById('b-newgame')!.onclick = () => {
    if (confirm('판과 손패를 비우고 새 판을 시작할까요? (조각 횟수와 설정은 유지)')) app.newGame();
  };
  document.addEventListener('keydown', (e) => {
    if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'z' && !(e.target as HTMLElement).matches('input,textarea')) {
      e.preventDefault();
      document.getElementById('b-undo')!.click();
    }
  });
  app.renderAll();
  app.status(store.state.example ? '예시 상태입니다. 스크린샷을 붙여넣거나 판을 직접 입력하세요.' : '준비됐습니다.');
  if (app.handCount() > 0) app.scheduleSolve();
}

/** 솔버 / 시뮬레이터 탭. */
function initTabs(): void {
  const show = (tab: string) => {
    document.getElementById('helperView')!.hidden = tab !== 'helper';
    document.getElementById('simView')!.hidden = tab !== 'sim';
    for (const b of document.querySelectorAll<HTMLButtonElement>('.tabs [data-tab]')) b.classList.toggle('on', b.dataset.tab === tab);
    try {
      localStorage.setItem('moamoa-tab', tab);
    } catch {
      /* 무시 */
    }
    if (tab === 'sim') renderSim();
  };
  for (const b of document.querySelectorAll<HTMLButtonElement>('.tabs [data-tab]')) b.onclick = () => show(b.dataset.tab!);
  let saved = 'helper';
  try {
    saved = localStorage.getItem('moamoa-tab') ?? 'helper';
  } catch {
    /* 무시 */
  }
  show(saved === 'sim' ? 'sim' : 'helper');
}

export const TRAINED_LABEL = '학습됨 (weights/trained.json)';

/** 학습된 가중치(weights/trained.json)를 기본으로 쓴다. 사용자가 직접 고른 가중치(파일·수정·기본 선택)는 건드리지 않고,
 *  이전에 자동 로드한 것이면 파일이 바뀌었을 때 새 값으로 갱신한다. */
async function loadTrainedWeightsIfDefault(): Promise<void> {
  const s = store.state.settings;
  const autoLoaded = s.weights !== null && s.weightsLabel === TRAINED_LABEL;
  if (s.weightsLabel === '기본(선택)' || (s.weights !== null && !autoLoaded)) return;
  try {
    const res = await fetch('./weights/trained.json', { cache: 'no-store' });
    if (!res.ok) return;
    const j = (await res.json()) as Record<string, number>;
    if (typeof j !== 'object' || !('filled' in j)) return;
    if (autoLoaded && JSON.stringify(j) === JSON.stringify(s.weights)) return;
    s.weights = j;
    s.weightsLabel = TRAINED_LABEL;
    store.save();
    if (autoLoaded) app.status('학습 가중치가 새 버전(weights/trained.json)으로 갱신되었습니다.', 'ok');
  } catch {
    /* 오프라인이거나 파일이 없으면 기본값 유지 */
  }
}

void boot();
