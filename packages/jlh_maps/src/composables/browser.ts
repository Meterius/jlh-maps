import { computed } from 'vue'

export enum Browser {
  Chrome = 'Chrome',
  Firefox = 'Firefox',
  Safari = 'Safari',
  Edge = 'Edge',
  Opera = 'Opera',
  InternetExplorer = 'Internet Explorer',
  Unknown = 'Unknown',
}

export function useBrowser() {
  const userAgent = typeof navigator !== 'undefined' ? navigator.userAgent : ''

  const browser = computed<Browser>(() => {
    if (/Edg\//.test(userAgent)) return Browser.Edge
    if (/OPR\//.test(userAgent) || /Opera/.test(userAgent)) return Browser.Opera
    if (/Firefox\//.test(userAgent)) return Browser.Firefox
    if (/Chrome\//.test(userAgent)) return Browser.Chrome
    if (/Safari\//.test(userAgent)) return Browser.Safari
    if (/MSIE|Trident/.test(userAgent)) return Browser.InternetExplorer

    return Browser.Unknown
  })

  return {
    browser,
    userAgent,
  }
}
