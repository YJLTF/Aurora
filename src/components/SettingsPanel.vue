<script setup lang="ts">
import { reactive, watch } from "vue";
import type { Settings } from "../types";

const props = defineProps<{ settings: Settings; appVersion: string }>();

const emit = defineEmits<{
  (e: "save", settings: Settings): void;
  (e: "close"): void;
  (e: "openDir"): void;
  (e: "openVscodeDir"): void;
  (e: "checkUpdate"): void;
}>();

const form = reactive<Settings>({ ...props.settings });

watch(
  () => props.settings,
  (s) => Object.assign(form, s),
  { deep: true, immediate: true },
);

function save() {
  emit("save", {
    downloadDir: form.downloadDir.trim(),
    githubApiBase: form.githubApiBase.trim() || "https://api.github.com",
    downloadProxy: form.downloadProxy.trim(),
    githubToken: form.githubToken.trim(),
    vscodeDir: form.vscodeDir.trim(),
    autoCheckSelf: form.autoCheckSelf,
  });
}
</script>

<template>
  <div class="overlay" @click.self="emit('close')">
    <div class="dialog narrow" role="dialog" aria-modal="true" aria-label="设置">
      <header class="dlg-head">
        <h2>设置</h2>
        <button class="btn ghost iconbtn" title="关闭" @click="emit('close')">✕</button>
      </header>

      <div class="dlg-body">
        <label class="field">
          <span class="flabel">下载目录</span>
          <div class="inrow">
            <input v-model="form.downloadDir" spellcheck="false" />
            <button class="btn ghost sm" @click="emit('openDir')">打开</button>
          </div>
          <span class="fhint">升级包保存位置</span>
        </label>

        <label class="field">
          <span class="flabel">VSCode 备份目录</span>
          <div class="inrow">
            <input v-model="form.vscodeDir" class="mono" spellcheck="false" />
            <button class="btn ghost sm" @click="emit('openVscodeDir')">打开</button>
          </div>
          <span class="fhint">存放离线 .vsix 的目录（含子文件夹），留空则默认为下载目录下的 vscode</span>
        </label>

        <label class="field">
          <span class="flabel">GitHub API 地址</span>
          <input v-model="form.githubApiBase" class="mono" spellcheck="false" />
          <span class="fhint">API 访问不畅时可换成镜像，如 https://gh-api.example.com</span>
        </label>

        <label class="field">
          <span class="flabel">GitHub 下载加速前缀（可选）</span>
          <input v-model="form.downloadProxy" class="mono" placeholder="https://gh-proxy.com/" spellcheck="false" />
          <span class="fhint">只对 github.com 的安装包直链生效，留空则直连</span>
        </label>

        <label class="field">
          <span class="flabel">GitHub Token（可选）</span>
          <input v-model="form.githubToken" type="password" class="mono" spellcheck="false" />
          <span class="fhint">未认证 API 限 60 次/小时；检测频繁或仓库较多时建议填写</span>
        </label>

        <label class="inline-check">
          <input v-model="form.autoCheckSelf" type="checkbox" />
          <span>启动时自动检查 Aurora 更新</span>
        </label>

        <div class="field">
          <span class="flabel">Aurora 版本</span>
          <div class="inrow">
            <span class="ver-show mono">v{{ appVersion || "…" }}</span>
            <button class="btn ghost sm" @click="emit('checkUpdate')">检查更新</button>
          </div>
          <span class="fhint">从 GitHub Releases 获取新版本，可下载安装包手动升级</span>
        </div>
      </div>

      <footer class="dlg-foot">
        <span class="spacer"></span>
        <button class="btn ghost" @click="emit('close')">取消</button>
        <button class="btn primary" @click="save">保存</button>
      </footer>
    </div>
  </div>
</template>
