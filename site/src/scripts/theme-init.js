/* Inlined in <head>, blocking, before the first paint. Kept small on purpose — every byte
   here is repeated in every page. The rationale is in `src/layouts/Base.astro`. */
(function () {
  var root = document.documentElement;
  var stored = null;
  try {
    stored = localStorage.getItem("quotadeck-theme");
  } catch (error) {
    /* Storage blocked: the choice is not remembered, the system theme still applies. */
  }
  if (stored === "light" || stored === "dark") root.setAttribute("data-theme", stored);
  root.setAttribute("data-js", "on");
})();
