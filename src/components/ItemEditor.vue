<script setup lang="ts">
import { computed, reactive, watch } from "vue";
import type { SoftItem, Source } from "../types";

const props = defineProps<{ item: SoftItem | null }>();

const emit = defineEmits<{
  (e: "save", item: SoftItem): void;
  (e: "delete", id: string): void;
  (e: "close"): void;
}>();

const form = reactive({
  name: "",
  icon: "📦",
  homepage: "",
  installedVersion: "",
  notes: "",
  type: "github" as "github" | "html",
  repo: "",
  checkUrl: "",
  versionRegex: "",
  downloadTemplate: "",
});

watch(
  () => props.item,
  (it) => {
    form.name = it?.name ?? "";
    form.icon = it?.icon || "📦";
    form.homepage = it?.homepage ?? "";
    form.installedVersion = it?.installedVersion ?? "";
    form.notes = it?.notes ?? "";
    if (it?.source.type === "github") {
      form.type = "github";
      form.repo = it.source.repo;
      form.checkUrl = "";
      form.versionRegex = "";
      form.downloadTemplate = "";
    } else if (it?.source.type === "html") {
      form.type = "html";
      form.checkUrl = it.source.checkUrl;
      form.versionRegex = it.source.versionRegex;
      form.downloadTemplate = it.source.downloadTemplate;
      form.repo = "";
    } else {
      form.type = "github";
      form.repo = "";
      form.checkUrl = "";
      form.versionRegex = "";
      form.downloadTemplate = "";
    }
  },
  { immediate: true },
);

function normalize(s: Source): string {
  return s.type === "github"
    ? `g|${s.repo.trim()}`
    : `h|${s.checkUrl.trim()}|${s.versionRegex.trim()}`;
}

const errors = computed(() => {
  const e: Partial<Record<string, string>> = {};
  if (!form.name.trim()) e.name = "必填";
  if (form.type === "github") {
    if (!/^[A-Za-z0-9_.-]+\/[A-Za-z0-9_.-]+$/.test(form.repo.trim()))
      e.repo = "格式应为 owner/name，如 wezterm/wezterm";
  } else {
    if (!form.checkUrl.trim()) e.checkUrl = "必填";
    else if (!/^https?:\/\//i.test(form.checkUrl.trim()))
      e.checkUrl = "应以 http(s):// 开头";
    if (!form.versionRegex.trim()) e.versionRegex = "必填";
    else {
      try {
        new RegExp(form.versionRegex);
      } catch {
        e.versionRegex = "正则语法无效";
      }
    }
  }
  return e;
});

function buildSource(): Source {
  return form.type === "github"
    ? { type: "github", repo: form.repo.trim() }
    : {
        type: "html",
        checkUrl: form.checkUrl.trim(),
        versionRegex: form.versionRegex.trim(),
        downloadTemplate: form.downloadTemplate.trim(),
      };
}

function save() {
  if (Object.keys(errors.value).length) return;
  const src = buildSource();
  const existing = props.item;
  const base: SoftItem = existing ?? {
    id: "",
    name: "",
    icon: "📦",
    source: src,
    homepage: "",
    notes: "",
    installedVersion: "",
    latestVersion: "",
    releaseUrl: "",
    assets: [],
    suggested: 0,
    checkedAt: 0,
    lastError: "",
  };
  const sourceChanged = normalize(base.source) !== normalize(src);
  emit("save", {
    ...base,
    name: form.name.trim(),
    icon: form.icon.trim() || "📦",
    homepage: form.homepage.trim(),
    installedVersion: form.installedVersion.trim(),
    notes: form.notes.trim(),
    source: src,
    // 来源变了之后，旧的检测结果不再可信，一并清掉
    latestVersion: sourceChanged ? "" : base.latestVersion,
    releaseUrl: sourceChanged ? "" : base.releaseUrl,
    assets: sourceChanged ? [] : base.assets,
    suggested: sourceChanged ? 0 : base.suggested,
    checkedAt: sourceChanged ? 0 : base.checkedAt,
    lastError: sourceChanged ? "" : base.lastError,
  });
}
</script>

<template>
  <div class="overlay" @click.self="emit('close')">
    <div class="dialog" role="dialog" aria-modal="true" aria-label="编辑软件">
      <header class="dlg-head">
        <h2>{{ item ? "编辑软件" : "添加软件" }}</h2>
        <button class="btn ghost iconbtn" title="关闭" @click="emit('close')">✕</button>
      </header>

      <div class="dlg-body">
        <div class="frow two">
          <label class="field">
            <span class="flabel">名称</span>
            <input v-model="form.name" placeholder="如 wezterm" spellcheck="false" />
            <span v-if="errors.name" class="ferr">{{ errors.name }}</span>
          </label>
          <label class="field narrow">
            <span class="flabel">图标</span>
            <input v-model="form.icon" maxlength="4" class="center" />
          </label>
        </div>

        <div class="field">
          <span class="flabel">检测方式</span>
          <div class="seg">
            <button
              type="button"
              :class="{ on: form.type === 'github' }"
              @click="form.type = 'github'"
            >
              GitHub Releases
            </button>
            <button
              type="button"
              :class="{ on: form.type === 'html' }"
              @click="form.type = 'html'"
            >
              页面 / 接口解析
            </button>
          </div>
        </div>

        <template v-if="form.type === 'github'">
          <label class="field">
            <span class="flabel">仓库</span>
            <input v-model="form.repo" placeholder="owner/name，如 wezterm/wezterm" spellcheck="false" />
            <span class="fhint">通过 GitHub API 读取最新 Release 与安装包列表</span>
            <span v-if="errors.repo" class="ferr">{{ errors.repo }}</span>
          </label>
        </template>
        <template v-else>
          <label class="field">
            <span class="flabel">检查地址</span>
            <input v-model="form.checkUrl" placeholder="https://example.com/download" spellcheck="false" />
            <span class="fhint">页面的 HTML 或返回 JSON 的接口地址都可以</span>
            <span v-if="errors.checkUrl" class="ferr">{{ errors.checkUrl }}</span>
          </label>
          <label class="field">
            <span class="flabel">版本正则</span>
            <input v-model="form.versionRegex" class="mono" placeholder='siyuan-([0-9][0-9.]*)-win\.exe' spellcheck="false" />
            <span class="fhint">第一个捕获组会作为版本号，如 (1.2.3)</span>
            <span v-if="errors.versionRegex" class="ferr">{{ errors.versionRegex }}</span>
          </label>
          <label class="field">
            <span class="flabel">下载直链模板（可选）</span>
            <input v-model="form.downloadTemplate" class="mono" placeholder="https://…/setup-{version}-win.exe" spellcheck="false" />
            <span class="fhint">用 {version} 占位最新版本号，填写后可直接下载</span>
          </label>
        </template>

        <div class="frow two tail">
          <label class="field">
            <span class="flabel">本地已装版本</span>
            <input v-model="form.installedVersion" class="mono" placeholder="留空表示未登记" spellcheck="false" />
          </label>
          <label class="field">
            <span class="flabel">主页（可选）</span>
            <input v-model="form.homepage" placeholder="https://…" spellcheck="false" />
          </label>
        </div>

        <label class="field">
          <span class="flabel">备注（可选）</span>
          <textarea v-model="form.notes" rows="2" placeholder="这台机器装的是便携版 / 需要手动安装…"></textarea>
        </label>
      </div>

      <footer class="dlg-foot">
        <button
          v-if="item"
          class="btn danger-ghost"
          @click="emit('delete', item.id)"
        >
          删除
        </button>
        <span class="spacer"></span>
        <button class="btn ghost" @click="emit('close')">取消</button>
        <button class="btn primary" :disabled="!!Object.keys(errors).length" @click="save">
          保存
        </button>
      </footer>
    </div>
  </div>
</template>
