const app = document.querySelector('#app')
const navItems = document.querySelectorAll('.nav-item')
const pages = document.querySelectorAll('.page')

navItems.forEach((item) => {
  item.addEventListener('click', () => {
    const target = item.dataset.page

    navItems.forEach((button) => button.classList.remove('active'))
    pages.forEach((page) => page.classList.remove('active'))

    item.classList.add('active')
    document.getElementById(target).classList.add('active')
  })
})

document.querySelector('#collapse').addEventListener('click', () => {
  app.classList.toggle('collapsed')
})

document
  .querySelectorAll('.mode-switch button, .tab-pair button')
  .forEach((button) => {
    button.addEventListener('click', () => {
      button.parentElement
        .querySelectorAll('button')
        .forEach((item) => item.classList.remove('active'))
      button.classList.add('active')
    })
  })

document.querySelectorAll('.proxy-node').forEach((node) => {
  node.addEventListener('click', () => {
    document
      .querySelectorAll('.proxy-node')
      .forEach((item) => item.classList.remove('selected'))
    node.classList.add('selected')
  })
})
