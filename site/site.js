(() => {
  const onPages = location.hostname.endsWith("github.io");
  const owner = onPages ? location.hostname.split(".")[0] : "";
  const repo = onPages ? (location.pathname.split("/").filter(Boolean)[0] || `${owner}.github.io`) : "";
  const repository = owner && repo ? `https://github.com/${owner}/${repo}` : "https://github.com/";
  const releases = `${repository}/releases`;

  document.querySelectorAll("[data-repo-link]").forEach((link) => { link.href = repository; });
  document.querySelectorAll("[data-releases-link]").forEach((link) => { link.href = releases; });
  document.querySelectorAll("[data-issues-link]").forEach((link) => { link.href = `${repository}/issues`; });
  document.querySelectorAll("[data-source-link]").forEach((link) => { link.href = `${repository}#build-from-source`; });

  const platform = /Mac/.test(navigator.platform) ? "mac" : /Win/.test(navigator.platform) ? "windows" : "linux";
  document.querySelector(`[data-platform="${platform}"]`)?.classList.add("current");
  const primary = document.querySelector("[data-download-link]");
  if (primary) {
    primary.textContent = `Download for ${platform === "mac" ? "macOS" : platform[0].toUpperCase() + platform.slice(1)} ↓`;
    primary.href = "#install";
  }

  if (!owner || !repo) return;
  fetch(`https://api.github.com/repos/${owner}/${repo}/releases/latest`, { headers: { Accept: "application/vnd.github+json" } })
    .then((response) => response.ok ? response.json() : Promise.reject())
    .then((release) => {
      const assets = release.assets || [];
      const patterns = {
        windows: /setup.*\.exe$|\.msi$/i,
        mac: /\.dmg$/i,
        linux: /\.AppImage$|\.deb$/i
      };
      Object.entries(patterns).forEach(([key, pattern]) => {
        const asset = assets.find((item) => pattern.test(item.name));
        const link = document.querySelector(`[data-asset="${key}"]`);
        if (asset && link) link.href = asset.browser_download_url;
      });
      const note = document.querySelector("[data-release-note]");
      if (note) note.textContent = `${release.tag_name} · Free community builds for Windows, macOS and Linux.`;
    })
    .catch(() => {
      document.querySelectorAll("[data-asset]").forEach((link) => { link.href = releases; });
    });
})();
