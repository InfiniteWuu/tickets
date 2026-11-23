<script setup>
import { computed, onBeforeUnmount, reactive, ref } from "vue";
import { Message } from "@arco-design/web-vue";
import QRCode from "qrcode";
import { runtime, saveAccount, deleteAccount, setDefaultAccount, reloadAccounts, errorText, desktop, call } from "../services/runtime";
import { platforms, formatTime } from "../services/platforms";
import UiIcon from "./common/UiIcon.vue";

const platform = ref("dm");
const accounts = computed(() => runtime.accounts.items.filter(item => item.platform === platform.value));
const form = reactive({ id: "", name: "", cookie: "" });
const showCookie = ref(false);
const error = ref("");

const loginSession = ref(null);
const qrImage = ref("");
const loginBusy = ref(false);
const loginError = ref("");
let loginTimer = null;
let loginGeneration = 0;

const qrMessage = computed(() => {
    const session = loginSession.value;
    if (!session) return "";
    if (session.status === "waiting") return "请使用手机大麦 App 扫描二维码登录";
    if (session.status === "scanned") return "已扫码，请在手机上确认登录";
    if (session.status === "expired") return "二维码已过期，请点击“重新获取二维码”";
    if (session.status === "error") return "扫码暂时中断，请重新获取二维码";
    return "";
});

function reset() { Object.assign(form, { id: "", name: "", cookie: "" }); showCookie.value = false; error.value = ""; }
function selectPlatform(id) { platform.value = id; reset(); cancelQrLogin(); }
function edit(account) { Object.assign(form, { id: account.id, name: account.name, cookie: account.cookie }); showCookie.value = false; error.value = ""; }
function save() {
    error.value = "";
    try { saveAccount({ ...form, platform: platform.value }); reset(); Message.success("账号已保存，购票和监控可直接选用"); }
    catch (value) { error.value = errorText(value); }
}
function remove(id) {
    try { deleteAccount(id); if (form.id === id) reset(); Message.success("账号已删除"); }
    catch (value) { error.value = errorText(value); }
}
function makeDefault(id) {
    try { setDefaultAccount(id); Message.success("已设为该平台默认账号"); }
    catch (value) { error.value = errorText(value); }
}

async function startDmQrLogin() {
    if (!desktop || loginBusy.value || platform.value !== "dm") return;
    loginBusy.value = true;
    loginError.value = "";
    clearTimeout(loginTimer);
    const generation = ++loginGeneration;
    try {
        const value = await call("dm_qrcode_generate");
        if (generation !== loginGeneration) return;
        const qrImageData = await QRCode.toDataURL(value.qrcodeContent, { width: 240, margin: 1 });
        if (generation !== loginGeneration) return;
        loginSession.value = { status: "waiting", message: "", lgToken: value.lgToken, t: value.t, cookie2: "", returnUrl: "", st: "" };
        qrImage.value = qrImageData;
        scheduleDmPoll(value.lgToken, value.t, generation);
    } catch (err) {
        if (generation === loginGeneration) loginError.value = errorText(err);
    } finally { loginBusy.value = false; }
}

async function pollDmQrLogin(lgToken, t, generation) {
    if (generation !== loginGeneration || loginBusy.value) return;
    loginBusy.value = true;
    try {
        const value = await call("dm_qrcode_poll", { lgToken, t });
        if (generation !== loginGeneration) return;
        loginSession.value = { status: value.status, message: value.message || "", lgToken, t, cookie2: value.cookie2 || "", returnUrl: value.returnUrl || "", st: value.st || "" };
        if (value.status === "confirmed") {
            clearTimeout(loginTimer);
            await finishDmExchange(value.cookie2, value.returnUrl, value.st, generation);
        } else if (["waiting", "scanned"].includes(value.status)) {
            scheduleDmPoll(lgToken, t, generation);
        }
    } catch (err) {
        if (generation === loginGeneration) {
            loginSession.value = { ...(loginSession.value || {}), status: "error" };
            loginError.value = errorText(err);
        }
    } finally { loginBusy.value = false; }
}

async function finishDmExchange(cookie2, returnUrl, st, generation) {
    if (!cookie2 || !returnUrl || !st) {
        loginSession.value = { ...(loginSession.value || {}), status: "error" };
        loginError.value = "未获取到登录凭证，请重新扫码";
        return;
    }
    loginBusy.value = true;
    try {
        const value = await call("dm_qrcode_exchange", { cookie2, returnUrl, st });
        if (generation !== loginGeneration) return;
        qrImage.value = "";
        loginSession.value = null;
        form.id = "";
        form.cookie = value.cookie;
        form.name = value.uid ? `大麦 扫码账号 ${value.uid}` : `大麦 扫码账号 ${Date.now() % 100000}`;
        showCookie.value = false;
        Message.success("扫码登录成功，Cookie 已自动填入，确认名称后点击“保存账号”");
    } catch (err) {
        if (generation === loginGeneration) {
            loginSession.value = { ...(loginSession.value || {}), status: "error" };
            loginError.value = errorText(err);
        }
    } finally { loginBusy.value = false; }
}

function scheduleDmPoll(lgToken, t, generation) {
    clearTimeout(loginTimer);
    loginTimer = setTimeout(() => pollDmQrLogin(lgToken, t, generation), 2000);
}

function cancelQrLogin() {
    loginGeneration++;
    clearTimeout(loginTimer);
    loginSession.value = null;
    qrImage.value = "";
    loginError.value = "";
    loginBusy.value = false;
}

onBeforeUnmount(cancelQrLogin);
</script>

<template>
    <section id="account-settings" class="panel settings-panel account-settings">
        <div class="section-heading"><span class="section-icon"><UiIcon name="user" /></span><div><h2>账号与 Cookie</h2><p>保存一次，购票和多个监控共用</p></div></div>
        <div class="segmented-control" aria-label="管理账号平台"><button v-for="(meta, id) in platforms" :key="id" :class="{ selected: platform === id }" :aria-pressed="platform === id" @click="selectPlatform(id)">{{ meta.name }}</button></div>
        <div v-if="runtime.accountError" class="notice error" role="alert"><span>{{ runtime.accountError }}</span><button class="text-button" @click="reloadAccounts">重新读取</button></div>
        <div class="account-list">
            <article v-for="account in accounts" :key="account.id" class="account-row">
                <div class="account-description"><strong>{{ account.name }}</strong><span v-if="runtime.accounts.defaults[platform] === account.id" class="pill">默认</span><small>Cookie 已保存 · {{ formatTime(account.updatedAt) }}</small></div>
                <div class="account-actions"><button v-if="runtime.accounts.defaults[platform] !== account.id" class="text-button" @click="makeDefault(account.id)">设为默认</button><button class="text-button" @click="edit(account)">编辑</button><button class="text-button" @click="remove(account.id)">删除</button></div>
            </article>
            <p v-if="!accounts.length" class="field-hint">尚无{{ platforms[platform].name }}账号，在下方保存 Cookie 后即可使用。</p>
        </div>
        <div class="section-rule"></div>
        <form @submit.prevent="save">
            <div class="label-row"><h3>{{ form.id ? '更新账号' : '添加账号' }}</h3><button v-if="form.id" type="button" class="text-button" @click="reset">取消编辑</button></div>
            <div class="field space-top"><label class="field-label" for="account-name">账号名称</label><input id="account-name" class="text-input" v-model="form.name" maxlength="60" placeholder="例如：我的常用账号" autocomplete="off" /></div>
            <div class="label-row space-top"><label class="field-label" for="account-cookie">账号 Cookie</label><button type="button" class="text-button" @click="showCookie = !showCookie">{{ showCookie ? '隐藏' : '显示' }}</button></div>
            <textarea id="account-cookie" class="text-input cookie-input" :class="{ concealed: !showCookie }" rows="4" v-model="form.cookie" placeholder="粘贴已登录账号的完整 Cookie" autocomplete="off" spellcheck="false"></textarea>
            <p class="field-hint">Cookie 以明文保存在当前设备的统一账号库中。账号单独保存，无需再点击页面顶部的保存设置。保存只检查格式，登录是否有效以平台查询结果为准。</p>
            <p class="field-hint">更新或删除账号后，已启动的任务继续使用启动时的 Cookie；新查询和新任务使用当前账号。Cookie 过期时，在这里更新一次即可。</p>
            <div v-if="platform === 'dm'" class="space-top">
                <button type="button" class="button secondary small" :disabled="!desktop || loginBusy" @click="startDmQrLogin">{{ loginBusy ? '正在获取二维码…' : '扫码登录（自动获取 Cookie）' }}</button>
                <p v-if="!desktop" class="field-hint">扫码登录需要在桌面应用中使用。</p>
            </div>
            <div v-if="loginSession" class="qr-login">
                <img v-if="qrImage" :src="qrImage" alt="大麦登录二维码" width="240" height="240" />
                <p class="qr-login-message" role="status">{{ qrMessage }}</p>
                <div class="button-row">
                    <button v-if="['expired', 'error'].includes(loginSession.status)" type="button" class="button secondary small" :disabled="loginBusy" @click="startDmQrLogin">重新获取二维码</button>
                    <button type="button" class="text-button" :disabled="loginBusy" @click="cancelQrLogin">取消扫码</button>
                </div>
            </div>
            <p v-if="loginError" class="inline-error" role="alert">{{ loginError }}</p>
            <p v-if="error" class="inline-error" role="alert">{{ error }}</p>
            <button class="button primary small space-top" type="submit" :disabled="!!runtime.accountError">{{ form.id ? '保存更新' : '保存账号' }}<UiIcon name="check" /></button>
        </form>
    </section>
</template>

<style scoped>
.account-settings { margin-bottom: 22px; }
.account-list { display: grid; gap: 12px; }
.account-row { display: flex; align-items: center; justify-content: space-between; gap: 12px; padding: 14px; border: 1px solid var(--border); border-radius: 8px; }
.account-description { min-width: 0; }
.account-description strong { font-size: 12px; overflow-wrap: anywhere; }
.account-description .pill { margin-left: 8px; }
.account-description small { display: block; color: var(--muted); font-size: 10px; line-height: 1.7; margin-top: 6px; }
.account-actions { display: flex; gap: 10px; flex-shrink: 0; flex-wrap: wrap; }
.qr-login { margin-top: 14px; padding: 16px; border: 1px solid var(--border); border-radius: 8px; }
.qr-login img { display: block; max-width: 100%; height: auto; margin: 0 auto; background: #fff; border-radius: 6px; }
.qr-login-message { font-size: 12px; line-height: 1.8; margin-top: 12px; }
@media (max-width: 700px) { .account-row { align-items: flex-start; flex-direction: column; } }
</style>
