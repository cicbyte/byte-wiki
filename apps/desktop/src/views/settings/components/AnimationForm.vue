<script setup lang="ts">
/**
 * 动画设置（外观 → 动画）
 * 偏好状态与持久化都在 useMotion，这里绑定其 reactive motionPrefs
 * （改动即时生效并写入 localStorage）。速度/位移用分段控件，其余用开关。
 */
import { computed } from "vue";
import { useMotionPrefs, type MotionSpeed, type MotionOffset } from "@/composables/useMotion";

const { motionPrefs } = useMotionPrefs();

const speedOptions = [
  { label: "快", value: "fast" },
  { label: "正常", value: "normal" },
  { label: "慢", value: "slow" },
];
const offsetOptions = [
  { label: "无", value: "none" },
  { label: "轻微", value: "light" },
  { label: "明显", value: "strong" },
];

// a-segmented 的 value 是 string|number，用 computed 收窄回枚举类型
const speedSel = computed({
  get: () => motionPrefs.speed,
  set: (v: string | number) => { motionPrefs.speed = v as MotionSpeed; },
});
const offsetSel = computed({
  get: () => motionPrefs.offset,
  set: (v: string | number) => { motionPrefs.offset = v as MotionOffset; },
});
</script>

<template>
  <div class="anim-form">
    <a-divider orientation="left" plain>基本</a-divider>
    <div class="anim-row">
      <div class="anim-info">
        <div class="anim-label">启用动画</div>
        <div class="anim-desc">总开关。关闭后所有过渡与入场动画瞬时完成。</div>
      </div>
      <a-switch v-model:checked="motionPrefs.enabled" />
    </div>
    <div class="anim-row">
      <div class="anim-info">
        <div class="anim-label">回弹效果</div>
        <div class="anim-desc">入场带轻微弹性回弹；关闭则平滑收尾（无 overshoot）。</div>
      </div>
      <a-switch v-model:checked="motionPrefs.bounce" :disabled="!motionPrefs.enabled" />
    </div>

    <a-divider orientation="left" plain>节奏</a-divider>
    <div class="anim-row">
      <div class="anim-info">
        <div class="anim-label">动画速度</div>
        <div class="anim-desc">整体快慢。觉得「弹太久」可调快。</div>
      </div>
      <a-segmented v-model:value="speedSel" :options="speedOptions" :disabled="!motionPrefs.enabled" />
    </div>
    <div class="anim-row">
      <div class="anim-info">
        <div class="anim-label">入场位移</div>
        <div class="anim-desc">元素入场时的上移距离。「无」即纯淡入。</div>
      </div>
      <a-segmented v-model:value="offsetSel" :options="offsetOptions" :disabled="!motionPrefs.enabled" />
    </div>

    <a-divider orientation="left" plain>细节</a-divider>
    <div class="anim-row">
      <div class="anim-info">
        <div class="anim-label">卡片错峰入场</div>
        <div class="anim-desc">仪表盘统计卡依次浮现；关闭则同时出现。</div>
      </div>
      <a-switch v-model:checked="motionPrefs.stagger" :disabled="!motionPrefs.enabled" />
    </div>
    <div class="anim-row">
      <div class="anim-info">
        <div class="anim-label">数字滚动</div>
        <div class="anim-desc">仪表盘统计数字从 0 滚到真值；关闭则直接显示终值。</div>
      </div>
      <a-switch v-model:checked="motionPrefs.countUp" :disabled="!motionPrefs.enabled" />
    </div>

    <p class="anim-note">
      系统开启了「减少动态效果」时，动画会自动降级为瞬时，优先于以上设置。
    </p>
  </div>
</template>

<style scoped>
.anim-form { padding: 8px 4px; }
.anim-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 16px;
  padding: 14px 0;
  border-bottom: 1px solid var(--border-light);
}
.anim-info { min-width: 0; }
.anim-label { font-size: 13px; font-weight: 600; color: var(--text-primary); }
.anim-desc { font-size: 12px; color: var(--text-tertiary); margin-top: 3px; line-height: 1.6; }
.anim-note { font-size: 11px; color: var(--text-disabled); margin-top: 16px; line-height: 1.6; }
</style>
