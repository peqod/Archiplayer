(() => {
  const owner = "peqod";
  const repo = "Archiplayer";
  const repository = `https://github.com/${owner}/${repo}`;
  const releases = `${repository}/releases`;

  document.querySelectorAll("[data-repo-link]").forEach((link) => { link.href = repository; });
  document.querySelectorAll("[data-releases-link]").forEach((link) => { link.href = releases; });
  document.querySelectorAll("[data-issues-link]").forEach((link) => { link.href = `${repository}/issues`; });
  document.querySelectorAll("[data-source-link]").forEach((link) => { link.href = `${repository}#build-from-source`; });

  const platform = /Mac/.test(navigator.platform) ? "mac" : /Win/.test(navigator.platform) ? "windows" : "linux";
  document.querySelector(`[data-platform="${platform}"]`)?.classList.add("current");
  const primary = document.querySelector("[data-download-link]");
  if (primary) {
    // Only Windows has a published build. Sending a Mac or Linux visitor to a
    // download that is not there is worse than saying so on the button.
    primary.textContent = platform === "windows"
      ? "Download for Windows ↓"
      : `${platform === "mac" ? "macOS" : "Linux"} builds coming soon`;
    primary.href = "#install";
  }

  fetch(`https://api.github.com/repos/${owner}/${repo}/releases/latest`, { headers: { Accept: "application/vnd.github+json" } })
    .then((response) => response.ok ? response.json() : Promise.reject())
    .then((release) => {
      const assets = release.assets || [];
      // No mac matcher while macOS distribution is paused: with no .dmg asset it
      // would leave the placeholder href pointing at the releases index.
      const patterns = {
        windows: /setup.*\.exe$|\.msi$/i
      };
      Object.entries(patterns).forEach(([key, pattern]) => {
        const asset = assets.find((item) => pattern.test(item.name));
        const link = document.querySelector(`[data-asset="${key}"]`);
        if (asset && link) link.href = asset.browser_download_url;
      });
      const note = document.querySelector("[data-release-note]");
      if (note) note.textContent = `${release.tag_name} · Early builds for Windows. macOS and Linux coming soon.`;
      const sha = assets.find((item) => /SHA256SUMS/i.test(item.name));
      const shaLink = document.querySelector("[data-sha256-link]");
      if (sha && shaLink) shaLink.href = sha.browser_download_url;
      // The VirusTotal permalink rides in the release body. It cannot be derived here from
      // the checksum file: api.github.com sends Access-Control-Allow-Origin, the asset
      // download host does not, so the browser can read this response but not that file.
      const vt = (release.body || "").match(/https:\/\/www\.virustotal\.com\/gui\/file\/[a-f0-9]{64}/i);
      const vtLink = document.querySelector("[data-vt-scan]");
      if (vt && vtLink) vtLink.href = vt[0];
    })
    .catch(() => {
      document.querySelectorAll("[data-asset]").forEach((link) => { link.href = releases; });
      const shaLink = document.querySelector("[data-sha256-link]");
      if (shaLink) shaLink.href = releases;
    });
})();
