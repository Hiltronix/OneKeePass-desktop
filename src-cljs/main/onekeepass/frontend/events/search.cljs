(ns onekeepass.frontend.events.search
  "Per-database, lock-aware toolbar search with debouncing and stale-reply guards."
  (:require [clojure.string :as str]
            [re-frame.core :refer [reg-event-db reg-event-fx reg-fx reg-sub dispatch subscribe]]
            [onekeepass.frontend.events.common :refer [active-db-key check-error]]
            [onekeepass.frontend.background :as bg]))

(def ^:private search-delay-ms 200)
(defonce ^:private search-timers (atom {}))

(defn focus-search-field []
  ;; Used by Ctrl-F and Edit > Search. An open modal should not be bypassed
  ;; by this function's callers.
  (when-not (.querySelector js/document ".MuiDialog-root")
    (when-let [input (.getElementById js/document "toolbar-search-input")]
      (when-not (.-disabled input)
      (.focus input)
      (.select input)
        (dispatch [:search/show-results])))))

(defn search-term-update [^js/Event e]
  (dispatch [:search/term-update (-> e .-target .-value)]))

(defn search-term-clear []
  (dispatch [:search/clear]))

(defn show-results []
  (dispatch [:search/show-results]))

(defn hide-results []
  (dispatch [:search/hide-results]))

(defn navigate-results [direction]
  (dispatch [:search/navigate direction]))

(defn open-result [uuid]
  (dispatch [:search/open-result uuid]))

(defn open-active-result [{:keys [selected-entry-id result]}]
  (when-let [uuid (or selected-entry-id (some-> result :entry-items first :uuid))]
    (open-result uuid)))

(defn search-data []
  (subscribe [:search/data]))

(defn- eligible-request? [db db-key term request-id]
  (and (= db-key (active-db-key db))
       (not (get-in db [db-key :locked]))
       (= request-id (get-in db [db-key :search :request-id]))
       (= term (get-in db [db-key :search :term]))
       (not (str/blank? term))))

(defn- next-request-id [_db _db-key]
  ;; Never reuse a request token after lock/close/reopen of the same DB path.
  ;; A late response from an older session must not match the new request.
  (str (random-uuid)))

(reg-event-fx
 :search/term-update
 (fn [{:keys [db]} [_ term]]
   (let [db-key (active-db-key db)
         allowed? (and db-key (not (get-in db [db-key :locked])))
         request-id (next-request-id db db-key)
         search? (and allowed? (not (str/blank? term)))]
     (if-not allowed?
       {}
       {:db (assoc-in db [db-key :search]
                      {:term term :request-id request-id
                       :popup-open? search? :pending? search?
                       :result nil :not-matched false
                       :error-text nil :selected-entry-id nil})
        :fx (when search? [[:search/schedule [db-key term request-id]]])}))))

(reg-fx
 :search/schedule
 (fn [[db-key term request-id]]
   (when-let [old (get @search-timers db-key)]
     (js/clearTimeout old))
   (let [timer (js/setTimeout
                (fn []
                  (swap! search-timers dissoc db-key)
                  (dispatch [:search/run db-key term request-id]))
                search-delay-ms)]
     (swap! search-timers assoc db-key timer))))

(reg-event-fx
 :search/run
 (fn [{:keys [db]} [_ db-key term request-id]]
   (if (eligible-request? db db-key term request-id)
     {:fx [[:search/backend [db-key term request-id]]]}
     {})))

(reg-fx
 :search/backend
 (fn [[db-key term request-id]]
   (bg/search-term
    db-key term
    (fn [response]
      (when-let [result
                 (check-error response
                              #(dispatch [:search/failed db-key term request-id %]))]
        (dispatch [:search/completed db-key term request-id result]))))))

(reg-event-db
 :search/completed
 (fn [db [_ db-key term request-id result]]
   (if (eligible-request? db db-key term request-id)
     (-> db
         (assoc-in [db-key :search :result] result)
         (assoc-in [db-key :search :pending?] false)
         (assoc-in [db-key :search :error-text] nil)
         (assoc-in [db-key :search :selected-entry-id] nil)
         (assoc-in [db-key :search :not-matched] (empty? (:entry-items result))))
     db)))

(reg-event-db
 :search/failed
 (fn [db [_ db-key term request-id error]]
   (if (eligible-request? db db-key term request-id)
     (-> db
         (assoc-in [db-key :search :result] nil)
         (assoc-in [db-key :search :pending?] false)
         (assoc-in [db-key :search :selected-entry-id] nil)
         (assoc-in [db-key :search :error-text] error))
     db)))

(reg-event-fx
 :search/show-results
 (fn [{:keys [db]} [_]]
   (let [db-key (active-db-key db)
         search (get-in db [db-key :search])
         term (:term search)
         search? (and db-key (not (get-in db [db-key :locked]))
                      (not (str/blank? term)))
         need-refresh? (and search? (not (:pending? search))
                            (nil? (:result search))
                            (nil? (:error-text search)))
         request-id (if need-refresh? (next-request-id db db-key)
                        (:request-id search))]
     {:db (cond-> db
            db-key (assoc-in [db-key :search :popup-open?] (boolean search?))
            need-refresh? (assoc-in [db-key :search :pending?] true)
            need-refresh? (assoc-in [db-key :search :request-id] request-id))
      :fx (when need-refresh?
            [[:search/schedule [db-key term request-id]]])})))

(reg-event-db
 :search/hide-results
 (fn [db [_]]
   (let [db-key (active-db-key db)]
     (if db-key (assoc-in db [db-key :search :popup-open?] false) db))))

(reg-event-db
 :search/clear
 (fn [db [_]]
   (let [db-key (active-db-key db)]
     (if db-key
       (assoc-in db [db-key :search]
                 {:term "" :request-id (next-request-id db db-key)
                  :popup-open? false :pending? false
                  :result nil :not-matched false :error-text nil
                  :selected-entry-id nil})
       db))))

(reg-event-db
 :search/navigate
 (fn [db [_ direction]]
   (let [db-key (active-db-key db)
         search (get-in db [db-key :search])
         items (get-in search [:result :entry-items])
         selected (:selected-entry-id search)]
     (if (and db-key (not (get-in db [db-key :locked]))
              (:popup-open? search) (seq items))
       (let [idx (first (keep-indexed (fn [i item]
                                        (when (= selected (:uuid item)) i)) items))
             next-idx (if (nil? idx)
                        (if (pos? direction) 0 (dec (count items)))
                        (mod (+ idx direction) (count items)))]
         (assoc-in db [db-key :search :selected-entry-id]
                   (:uuid (nth items next-idx))))
       db))))

(reg-event-fx
 :search/open-result
 (fn [{:keys [db]} [_ uuid]]
   (let [db-key (active-db-key db)
         search (get-in db [db-key :search])]
     (if (and db-key (not (get-in db [db-key :locked]))
              (:popup-open? search)
              (some #(= uuid (:uuid %)) (get-in search [:result :entry-items])))
       {:db (assoc-in db [db-key :search :popup-open?] false)
        :fx [[:dispatch [:entry-list/entry-selected-in-search-result uuid]]]}
       {}))))

(reg-sub
 :search/data
 (fn [db _]
   (get-in db [(active-db-key db) :search])))
