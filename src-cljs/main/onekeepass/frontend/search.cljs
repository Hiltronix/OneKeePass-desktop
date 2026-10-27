(ns onekeepass.frontend.search
  "Permanent toolbar search with an anchored, non-modal results panel."
  (:require [clojure.string :as str]
            [onekeepass.frontend.db-icons :refer [render-entry-icon]]
            [onekeepass.frontend.events.common :as cmn-events]
            [onekeepass.frontend.events.search :as sch-event]
            [onekeepass.frontend.mui-components :as m
             :refer [mui-alert mui-avatar mui-box mui-icon-button
                     mui-icon-clear-outlined mui-icon-search mui-input-adornment
                     mui-list-item-avatar mui-list-item-button mui-list-item-text
                     mui-paper mui-popper mui-stack mui-typography]]
            [onekeepass.frontend.translation :as t :refer-macros [tr-h]]
            [reagent.core :as r]))

(defn- result-row [db-key item selected-id]
  (let [uuid (:uuid item)]
    ^{:key uuid}
    [mui-list-item-button
     {:selected (= selected-id uuid)
      :data-search-selected (if (= selected-id uuid) "true" "false")
      :divider true
      ;; Keep keyboard focus on the search input so arrows/Enter remain active.
      :on-mouse-down #(.preventDefault %)
      :on-click (fn [_] (sch-event/open-result uuid))}
     [mui-list-item-avatar
      [mui-avatar {:variant "rounded" :sx {:border-radius 2}}
       [render-entry-icon {:db-key db-key
                           :icon-id (:icon-id item)
                           :custom-icon-uuid (:custom-icon-uuid item)}]]]
     [mui-list-item-text {:primary (:title item)
                          :secondary (:secondary-title item)}]]))

(defn- scroll-selected-result []
  ;; The panel is scrollable; keep keyboard selection visible.
  (js/requestAnimationFrame
   (fn [_]
     (when-let [el (.querySelector js/document
                                  "[data-search-selected='true']")]
       (.scrollIntoView el #js {:block "nearest" :inline "nearest"})))))

(defn- search-key-down [^js/Event event data]
  (let [key (.-key event)
        items (get-in data [:result :entry-items])]
    (case key
      "ArrowDown" (do (.preventDefault event)
                      (sch-event/navigate-results 1)
                      (scroll-selected-result))
      "ArrowUp" (do (.preventDefault event)
                    (sch-event/navigate-results -1)
                    (scroll-selected-result))
      "Enter" (when (seq items)
                (.preventDefault event)
                (sch-event/open-active-result data))
      "Escape" (do (.preventDefault event) (sch-event/hide-results))
      nil)))

(defn toolbar-search []
  (let [{:keys [term result error-text not-matched pending? popup-open? selected-entry-id]
         :as data} @(sch-event/search-data)
        db-key @(cmn-events/active-db-key)
        locked? @(cmn-events/locked?)
        entries (:entry-items result)
        results-visible? (and (not locked?) popup-open? (not (str/blank? term)))
        ;; The element is already mounted by the time a focused field can open
        ;; the popper. Anchor to its border rather than to the cursor/caret.
        anchor (when results-visible?
                 (.getElementById js/document "toolbar-search-anchor"))]
    [mui-box {:id "toolbar-search-anchor"
              :sx {:width {:xs 190 :sm 245 :md 295}
                   :maxWidth "35vw"
                   :minWidth 165}}
     [m/text-field
      {:id "toolbar-search-input"
       :aria-label "Search database entries"
       :placeholder "Search entries..."
       :variant "outlined"
       :size "small"
       :fullWidth true
       :disabled locked?
       :value (or term "")
       ;; Additional defense; the WebView2 general autofill switch is disabled.
       :autoComplete "off"
       :slotProps {:htmlInput {:autoComplete "off" :spellCheck false}
                   :input {:startAdornment
                           (r/as-element
                            [mui-input-adornment {:position "start"}
                             [mui-icon-search {:fontSize "small"}]])
                           :endAdornment
                           (when-not (str/blank? term)
                             (r/as-element
                              [mui-input-adornment {:position "end"}
                               [mui-icon-button
                                {:size "small"
                                 :aria-label "Clear search"
                                 :on-mouse-down #(.preventDefault %)
                                 :on-click (fn [_] (sch-event/search-term-clear))}
                                [mui-icon-clear-outlined {:fontSize "small"}]]]))}}
       :sx {:bgcolor "background.paper"
            :borderRadius 1
            "& .MuiInputBase-input" {:py "7px"}}
       :on-change sch-event/search-term-update
       :on-focus (fn [_] (sch-event/show-results))
       :on-blur (fn [_] (sch-event/hide-results))
       :on-key-down (fn [event] (search-key-down event data))}]
     [mui-popper
      {:open (boolean results-visible?)
       :anchorEl anchor
       :placement "bottom-end"
       :disablePortal false
       :sx {:zIndex 1500}}
      [mui-paper {:elevation 7
                  :dir (t/dir)
                  :sx {:width 375
                       :maxWidth "calc(100vw - 32px)"
                       :mt 0.5
                       :overflow "hidden"}}
       [mui-stack {:sx {:maxHeight "66.6667vh" :overflowY "auto"}}
        (cond
          error-text
          [mui-alert {:severity "error"} error-text]

          pending?
          [mui-typography {:variant "body2" :sx {:p 2}} "Searching..."]

          (seq entries)
          (into [:<>] (map (fn [item] (result-row db-key item selected-entry-id)) entries))

          (and not-matched (not (str/blank? term)))
          [mui-alert {:severity "info"} (tr-h "noMatchingEntryFound")]

          :else nil)]
       (when (seq entries)
         [mui-typography {:variant "caption"
                          :sx {:display "block" :px 2 :py 1}}
          (str (count entries) " matching entries · ↑↓ select · Enter open")])]]]))
