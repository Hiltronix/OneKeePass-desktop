(ns onekeepass.frontend.check-for-updates
  (:require
   [onekeepass.frontend.events.generic-dialogs :as gd-events]
   [onekeepass.frontend.mui-components :refer [mui-button mui-dialog
                                              mui-dialog-actions mui-dialog-content
                                              mui-dialog-title mui-typography]]
   [onekeepass.frontend.translation :as t]))

(defn check-for-updates-dialog-main []
  (let [{:keys [dialog-show]} @(gd-events/check-for-updates-dialog-data)]
    [mui-dialog {:open (boolean dialog-show)
                 :dir (t/dir)
                 :on-close (fn [_event reason]
                             (when (= reason "escapeKeyDown")
                               (gd-events/check-for-updates-dialog-close)))
                 :sx {"& .MuiDialog-paper" {:width "420px"}}}
     [mui-dialog-title "Check for Updates"]
     [mui-dialog-content
      [mui-typography "Not currently in use."]]
     [mui-dialog-actions
      [mui-button {:on-click gd-events/check-for-updates-dialog-close} "Close"]]]))
