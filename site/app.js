const languageButtons = document.querySelectorAll('[data-language]');

function setLanguage(language) {
  document.documentElement.lang = language;
  document.querySelectorAll('[data-i18n]').forEach(element => {
    element.innerHTML = messages[language][element.dataset.i18n];
  });
  languageButtons.forEach(button => {
    button.setAttribute('aria-pressed', String(button.dataset.language === language));
  });
}

setLanguage(siteLanguage.read('kakoi-lp-language'));
languageButtons.forEach(button =>
  button.addEventListener('click', () => {
    setLanguage(button.dataset.language);
    siteLanguage.save(button.dataset.language);
  })
);
