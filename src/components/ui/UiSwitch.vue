<script setup lang="ts">
/** 开关：代替复选框，v-model 绑定布尔值 */
const model = defineModel<boolean>({ required: true });
</script>

<template>
  <button
    type="button"
    role="switch"
    :aria-checked="model"
    class="relative inline-flex h-5 w-9 shrink-0 cursor-pointer items-center rounded-full border transition-colors duration-200 disabled:cursor-not-allowed disabled:opacity-50"
    :class="model ? 'border-transparent' : 'border-line bg-subtle'"
    :style="model ? { background: 'var(--md-grad)' } : undefined"
    @click="model = !model"
  >
    <span
      class="inline-block size-3.5 rounded-full shadow transition-transform duration-200"
      :class="model ? 'translate-x-[18px] bg-white' : 'translate-x-[2px] bg-faint'"
    />
  </button>
</template>
