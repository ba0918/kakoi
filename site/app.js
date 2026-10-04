const languageButtons = document.querySelectorAll('[data-language]');
const translatedElements = [...document.querySelectorAll('[data-ja]')].map(element => ({
  element,
  en: element.innerHTML,
  ja: element.dataset.ja
}));

function setLanguage(language) {
  const selected = language === 'ja' ? 'ja' : 'en';
  document.documentElement.lang = selected;
  translatedElements.forEach(item => { item.element.innerHTML = item[selected]; });
  languageButtons.forEach(button => {
    button.setAttribute('aria-pressed', String(button.dataset.language === selected));
  });
  document.title = selected === 'ja'
    ? 'kakoi — 境界を決める。CLIを動かす。'
    : 'kakoi — Define the boundary. Run your CLI.';
  try { localStorage.setItem('kakoi-lp-language', selected); } catch { /* File previews may restrict storage. */ }
}

let savedLanguage = 'en';
try { savedLanguage = localStorage.getItem('kakoi-lp-language') || 'en'; } catch { /* Keep the English first-visit default. */ }
setLanguage(savedLanguage);
languageButtons.forEach(button => button.addEventListener('click', () => setLanguage(button.dataset.language)));
