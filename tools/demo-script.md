# Qvault Demo Video Script

**Duration:** 45-55 seconds
**Resolution:** 1920x1080 (or Retina equivalent)
**Format:** MP4 (H.264), 30fps

## Recording Setup

### macOS Screen Recording
```bash
# Option 1: macOS built-in (Cmd+Shift+5, select window)
# Option 2: ffmpeg from command line
ffmpeg -f avfoundation -framerate 30 -i "1:none" -t 55 \
  -vf "scale=1920:1080" -c:v libx264 -preset fast \
  -pix_fmt yuv420p demo-raw.mov

# Trim and compress final version
ffmpeg -i demo-raw.mov -ss 00:00:02 -t 00:00:50 \
  -c:v libx264 -crf 23 -preset slow demo.mp4
```

## Storyboard

### Scene 1: Title Card (0-4s)
**Setup:** Create a simple title screen or start with the app already open.
- Show the Qvault window with the upload screen visible
- The queue dashboard should be empty/clean

### Scene 2: Upload Document (4-12s)
**Actions:**
1. Click the "Upload Document" button (or drag the example PDF from Finder)
2. Select `tools/example-contract.pdf` (the Professional Services Agreement)
3. Show the upload progress indicator briefly

**Key moment:** The file appears in the document list

### Scene 3: PII Scanning (12-20s)
**Actions:**
1. The document opens in the viewer automatically
2. Show the scanning progress indicator ("Scanning for PII...")
3. Wait for the scan to complete — PII highlights appear on the document

**Key moment:** Multiple colored highlights appear across the contract — names in yellow, SSNs in red, addresses in blue, phone numbers in green

### Scene 4: Review & Approve (20-38s)
**Actions:**
1. Pan/scroll through the document slowly to show highlighted PII
2. In the sidebar, show the grouped categories (Person Names, SSN, etc.)
3. Click on one entity to select it — show it highlighted on the document
4. Click the checkmark (approve) on 2-3 individual entities
5. Click "Approve All" button to approve remaining
6. Show the count change (pending → approved)

**Key moment:** The approve animation and sidebar status updates

### Scene 5: Export Clean Version (38-48s)
**Actions:**
1. Click "Export Clean" button in the sidebar
2. Show the export progress
3. Toast notification: "Exported successfully"

**Key moment:** Clean export completes

### Scene 6: End Card (48-55s)
**Show:** Either leave the app on screen with the exported file, or create an end card:
- "**Qvault** — Protect Client Privacy. Automatically."
- "qvault.tech"
- "$299/year — All features included"

## Tips for Recording

1. **Before recording:**
   - Close all other apps
   - Hide the macOS dock
   - Set desktop wallpaper to solid dark color
   - Resize the Qvault window to fill most of the screen
   - Make sure the example PDF is ready in a convenient location

2. **During recording:**
   - Move the mouse smoothly and deliberately
   - Pause briefly on key moments (highlight appearance, approve action)
   - Don't rush — viewers need time to register what's happening

3. **Post-processing:**
   - Add subtle zoom/pan effects on key moments (optional)
   - Add background music (royalty-free, subtle)
   - Trim dead time
   - Target: under 60 seconds, ideally 45-50s

## Quick Recording Command

```bash
# Start recording (press Ctrl+C to stop)
cd ~/projects/qvault
screencapture -v tools/demo-recording.mov
```
