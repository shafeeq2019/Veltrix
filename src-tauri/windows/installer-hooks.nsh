; Remove the native messaging registration created by the app (HKCU only).
!macro NSIS_HOOK_POSTUNINSTALL
  DeleteRegKey HKCU "Software\Google\Chrome\NativeMessagingHosts\app.veltrix"
  DeleteRegKey HKCU "Software\Chromium\NativeMessagingHosts\app.veltrix"
  DeleteRegKey HKCU "Software\Microsoft\Edge\NativeMessagingHosts\app.veltrix"
  DeleteRegKey HKCU "Software\Mozilla\NativeMessagingHosts\app.veltrix"
  ; Registration of the former name "OpenDownload Manager"
  DeleteRegKey HKCU "Software\Google\Chrome\NativeMessagingHosts\org.opendownload.manager"
  DeleteRegKey HKCU "Software\Chromium\NativeMessagingHosts\org.opendownload.manager"
  DeleteRegKey HKCU "Software\Microsoft\Edge\NativeMessagingHosts\org.opendownload.manager"
  DeleteRegKey HKCU "Software\Mozilla\NativeMessagingHosts\org.opendownload.manager"
!macroend
