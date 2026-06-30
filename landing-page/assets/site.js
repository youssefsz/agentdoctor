const navToggle = document.querySelector(".nav-toggle");
const siteNav = document.querySelector(".site-nav");

if (navToggle && siteNav) {
  navToggle.addEventListener("click", () => {
    const isOpen = siteNav.classList.toggle("open");
    navToggle.setAttribute("aria-expanded", String(isOpen));
  });

  document.addEventListener("click", (event) => {
    if (!siteNav.contains(event.target) && !navToggle.contains(event.target)) {
      siteNav.classList.remove("open");
      navToggle.setAttribute("aria-expanded", "false");
    }
  });
}

document.querySelectorAll("[data-tab-target]").forEach((tab) => {
  tab.addEventListener("click", () => {
    const targetId = tab.getAttribute("data-tab-target");
    const tabs = tab.closest(".tabs");
    const panelRoot = tabs?.nextElementSibling;
    const sourceUrl = tab.getAttribute("data-source-url");
    const sourceLink = tab.closest(".install-panel")?.querySelector("[data-source-link]");

    tabs?.querySelectorAll("[role='tab']").forEach((candidate) => {
      const isSelected = candidate === tab;
      candidate.classList.toggle("active", isSelected);
      candidate.setAttribute("aria-selected", String(isSelected));
    });

    panelRoot?.querySelectorAll("[role='tabpanel']").forEach((panel) => {
      const isSelected = panel.id === targetId;
      panel.classList.toggle("active", isSelected);
      panel.hidden = !isSelected;
    });

    if (sourceLink && sourceUrl) {
      sourceLink.href = sourceUrl;
    }
  });
});

document.querySelectorAll("[data-copy-target]").forEach((button) => {
  button.addEventListener("click", async () => {
    const targetId = button.getAttribute("data-copy-target");
    const target = targetId ? document.getElementById(targetId) : null;
    const command = target?.querySelector("code")?.textContent?.trim();

    if (!command || !navigator.clipboard) {
      return;
    }

    await navigator.clipboard.writeText(command);
    const originalText = button.textContent;
    button.textContent = "Copied";
    window.setTimeout(() => {
      button.textContent = originalText;
    }, 1600);
  });
});
