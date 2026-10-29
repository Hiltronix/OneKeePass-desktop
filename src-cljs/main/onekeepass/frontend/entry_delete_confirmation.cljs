(ns onekeepass.frontend.entry-delete-confirmation
  (:require [onekeepass.frontend.events.common :refer [get-in-key-db]]
            [onekeepass.frontend.events.entry-form-common :refer [entry-form-key]]
            [onekeepass.frontend.events.entry-form-ex :as form-events]
            [onekeepass.frontend.mui-components :refer [mui-button
                                                       mui-dialog
                                                       mui-dialog-actions
                                                       mui-dialog-content
                                                       mui-dialog-title
                                                       mui-stack
                                                       mui-typography]]
            [onekeepass.frontend.translation :as t]
            [re-frame.db :as rf-db]
            [reagent.core :as r]))

;; Keep this confirmation independent of the entry form's rendering and Re-frame
;; subscriptions. Only construct a MUI dialog while an entry is pending deletion.
(defonce ^:private pending-delete (r/atom nil))

(defn request-delete
  ([entry-id]
   (request-delete entry-id nil))
  ([entry-id known-title]
   (when entry-id
     (let [db @rf-db/app-db
           db-key (:current-db-file-name db)
           entries (get-in-key-db db [:entry-list :selected-entry-items])
           selected-entry (some #(when (= (:uuid %) entry-id) %) entries)
           form-data (get-in-key-db db [entry-form-key :data])
           title (or known-title
                     (:title selected-entry)
                     (when (= entry-id (:uuid form-data)) (:title form-data))
                     "")]
       (reset! pending-delete {:entry-id entry-id
                               :db-key db-key
                               :entry-title title})))))

(defn cancel-delete []
  (reset! pending-delete nil))

(defn- confirm-delete []
  (let [{:keys [entry-id db-key]} @pending-delete]
    (cancel-delete)
    ;; Reject a stale request if the active database changed while the dialog
    ;; was open. Deletion uses the current database in the existing handler.
    (when (and entry-id db-key
               (= db-key (:current-db-file-name @rf-db/app-db)))
      (form-events/entry-delete-start entry-id))))

(defn delete-confirmation-dialog []
  (when-let [{:keys [entry-title]} @pending-delete]
    [mui-dialog {:open true
                 :on-close (fn [_e reason]
                             (when (= reason "escapeKeyDown")
                               (cancel-delete)))
                 :on-click #(.stopPropagation ^js/Event %)}
     [mui-dialog-title "Delete entry"]
     [mui-dialog-content {:dividers true}
      [mui-stack {:spacing 1}
       [mui-typography {:sx {:font-weight 600}} (or entry-title "")]
       [mui-typography "Are you sure?"]]]
     [mui-dialog-actions
      [mui-button {:autoFocus true
                   :on-click cancel-delete} (t/lstr-bl 'cancel)]
      [mui-button {:on-click confirm-delete} (t/lstr-bl 'ok)]]]))
