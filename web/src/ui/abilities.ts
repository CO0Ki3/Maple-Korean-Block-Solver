// 점수·줄·배치 카운터·보유 능력 패널. 게임 화면과 다르면 직접 맞출 수 있다.
import { app } from '../app';
import { store } from '../store';
import { ABILITY_CAP } from '../types';

export function renderAbilities(): void {
  const el = document.getElementById('abilities')!;
  const g = app.game;
  const held = g.dots + g.rerolls;
  el.innerHTML = `
    <div class="counter"><span>점수</span><input type="number" id="in-score" value="${g.score}" min="0" step="1" /></div>
    <div class="counter"><span>제거한 줄</span><input type="number" id="in-lines" value="${g.lines}" min="0" step="1" /></div>
    <div class="counter"><span>다음 능력 획득까지</span><b>${app.nextSpawnIn()}번</b><button data-k="placements" data-d="-1" aria-label="배치 횟수 감소">−</button><button data-k="placements" data-d="1" aria-label="배치 횟수 증가">+</button><span class="small muted">(누적 배치 ${g.placements})</span></div>
    <div class="counter"><span>⊙ 점 찍기</span><button data-k="dots" data-d="-1">−</button><b>${g.dots}</b><button data-k="dots" data-d="1">+</button></div>
    <div class="counter"><span>⇄ 바꿔 뽑기</span><button data-k="rerolls" data-d="-1">−</button><b>${g.rerolls}</b><button data-k="rerolls" data-d="1">+</button></div>
    <div class="counter"><span>능력 보유</span><b>${held}/${ABILITY_CAP}</b>${held >= ABILITY_CAP ? '<span class="tag warn">가득 참: 새 아이콘이 생기지 않습니다</span>' : ''}</div>
    <div class="counter"><span>판 위 아이콘</span><b>${g.icons.length}/3</b>${g.icons.length >= 3 ? '<span class="tag info">다음 생성 시 가장 오래된 아이콘 소멸</span>' : ''}</div>
    ${g.over ? '<div class="banner warn">이 상태는 게임 종료 조건입니다 (놓을 곳 없음 + 능력 없음).</div>' : ''}
    <div class="small muted">게임 화면의 숫자와 다르면 여기서 맞춰주세요. 배치 7번마다 판에 능력 아이콘이 생기고, 그 줄을 지우면 능력을 얻습니다 (보유 7개면 생성·획득 없음).</div>`;
  for (const b of el.querySelectorAll<HTMLButtonElement>('[data-k]')) {
    b.onclick = () => {
      const k = b.dataset.k as 'placements' | 'dots' | 'rerolls';
      const d = +b.dataset.d!;
      app.mutate(() => {
        const gg = store.state.game;
        if (k === 'placements') gg.placements = Math.max(0, gg.placements + d);
        else {
          const other = k === 'dots' ? gg.rerolls : gg.dots;
          gg[k] = Math.max(0, Math.min(ABILITY_CAP - other, gg[k] + d));
        }
      });
    };
  }
  const score = el.querySelector<HTMLInputElement>('#in-score')!;
  const lines = el.querySelector<HTMLInputElement>('#in-lines')!;
  score.onchange = () => {
    store.state.game.score = Math.max(0, Math.round(+score.value || 0));
    store.save();
    app.simulatePlan(); // 계획 프레임의 after 상태가 옛 점수를 들고 있지 않게
    app.renderAll();
  };
  lines.onchange = () => {
    store.state.game.lines = Math.max(0, Math.round(+lines.value || 0));
    store.save();
    app.simulatePlan();
    app.renderAll();
  };
}
