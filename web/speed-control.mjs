// Front-end playback-rate selection; independent from SNES save-state contents.
// Keep the native 60.0988 Hz simulation step and the existing HD frame budget.
export const SPEED_LEVELS=Object.freeze([1,2,3]);
export const SPEED_NOTICE_KEY='dbz-ssd-update-2026-11-22-v56b-seen';

export function nextSpeed(current){
  const index=SPEED_LEVELS.indexOf(current);
  return SPEED_LEVELS[(index+1)%SPEED_LEVELS.length];
}

/**
 * Time scaled in the emulation scheduler, NOT in RAF / CSS animation.
 * At 1x the same 1-to-2 tick cap remains in place. In HD, expensive frames
 * still use a 1 tick cap: a slower phone may not attain requested 3x.
 * The backlog is deliberately bounded to prevent long catch-up stalls.
 */
export function speedFramePlan({gapMs,accumulatorMs,rate=1,averageTickMs=0,stepMs=1000/60.0988}){
  if(!SPEED_LEVELS.includes(rate))throw Error('不支援的遊戲倍速');
  if(!(Number.isFinite(stepMs)&&stepMs>0))throw Error('無效的模擬影格時間');
  const boundedGap=Math.max(0,Math.min(Number.isFinite(gapMs)?gapMs:0,stepMs*2));
  const previous=Number.isFinite(accumulatorMs)?Math.max(0,accumulatorMs):0;
  const budget=Math.min(stepMs*3,previous+boundedGap*rate);
  const heavy=averageTickMs>=stepMs*.90;
  const maxCatchup=heavy?1:rate===3?3:2;
  const ticks=Math.min(Math.floor((budget+1e-8)/stepMs),maxCatchup);
  // Never accumulate an unbounded backlog on a slow device.
  return {ticks,remainingMs:Math.max(0,budget-ticks*stepMs),maxCatchup};
}
