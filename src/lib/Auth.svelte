<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";

  let token = $state("");
  let busy = $state(false);
  let error = $state<string | null>(null);

  // No callback is needed. On success, the Rust side sets the status in
  // timer-state to "connected". App.svelte then swaps the view by itself.
  async function saveToken() {
    busy = true;
    error = null;
    try {
      await invoke("set_api_token", { token });
    } catch (e) {
      error = String(e);
    } finally {
      busy = false;
    }
  }
</script>

<main class="auth-wrap">
  <section class="card auth-card">
    <span class="auth-logo">⏱</span>
    <h1>Welcome to Trackfecta</h1>
    <p class="muted" style="margin:0;font-size:13.5px;line-height:1.5">
      Paste your Toggl&nbsp;Track API token to get started. You can find it on your
      <a
        class="link"
        href="https://track.toggl.com/profile"
        target="_blank"
        rel="noreferrer">profile page</a>.
      It is stored securely on this device, in your system keyring when one is
      available, otherwise in Trackfecta's private app data.
    </p>
    <input
      class="input"
      type="password"
      placeholder="API token"
      bind:value={token}
    />
    <button
      class="btn primary"
      disabled={!token.trim() || busy}
      onclick={saveToken}
    >
      {busy ? "Verifying…" : "Connect"}
    </button>
    {#if error}
      <p class="error">{error}</p>
    {/if}
    <p class="fineprint">
      Trackfecta is a beautiful, but unofficial client and is not affiliated with, endorsed by,
      or sponsored by Toggl™.
    </p>
  </section>
</main>
