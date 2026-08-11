# Add identity dialog

**Date:** 2026-08-11  
**Status:** Approved for implementation  
**Scope:** Contact handles table **Add** flow in `web/`

## Problem

**Add** inserts an inline edit row into the handles table. That is cramped for the Service dropdown (especially “Text message”) and mixes editing with the data grid.

## Goals

- **Add** opens a modal: Service select + Identity field, **Cancel** / **OK**.
- Same `add_handle` API as today; refresh the table on success.
- Remove the inline add row.

## Non-goals

- Edit-existing-handle dialog.
- Changing remove-identity confirm.

## Design

- Modal via `ModalShell` (title **Add identity**).
- Service: Text message / WhatsApp (default phone).
- Identity: text input, autofocus.
- **OK** disabled when identity empty or busy; Enter submits when enabled.
- Escape / dismiss closes when not busy.

## Files

- Create: `web/src/components/contactDrawer/AddIdentityDialog.tsx`
- Modify: `web/src/components/contactDrawer/ContactDrawerHandles.tsx`
