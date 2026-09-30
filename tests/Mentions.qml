// Actual composer and exact mention intent; no helper, relay or model.
import QtQuick
import Quickshell
import "plugin" as Buzz
ShellRoot {
  id: test
  property alias testedService: service
  Buzz.Service { id: service; autoConnect: false }
  Buzz.PanelContent { id: view; width: 1100; height: 1000; service: test.testedService }
  function findComposer(item) {
    if (item.objectName === "buzzComposer") return item
    for (var i=0;i<item.children.length;i++) { var found=findComposer(item.children[i]); if(found) return found }
    return null
  }
  Timer {
    interval: 100; running: true
    onTriggered: {
      try {
        function check(ok, text) { if(!ok) throw new Error(text) }
        service.beginSession()
        var room="11111111-1111-4111-8111-111111111111", other="22222222-2222-4222-8222-222222222222"
        var codex="a".repeat(64), duplicate="b".repeat(64), claude="c".repeat(64)
        service.catalogRooms=[{id:room,name:"Test",description:"",kind:"stream",participants:[],hidden:false},{id:other,name:"Other",description:"",kind:"stream",participants:[],hidden:false}]
        service.catalogState="ready"; service.selectedRoomId=room; service.connection="authenticated"
        service.recipientsSupported=true; service.sendSupported=true
        service.instanceId="mentions-test"; service.generation=1
        service.recipientsState="snapshot"; service.recipientsRoomId=room
        service.recipientEntries=[{key:codex,name:"Codex (isolated)"},{key:duplicate,name:"Codex (isolated)"},{key:claude,name:"Claude"}]
        var composer=findComposer(view)
        function type(text) { composer.text=text;composer.cursorPosition=text.length }
        type("@co")
        check(view.mentionMatches.length===2,"Duplicate names must remain separate choices")
        check(view.chooseMention(1),"Completion not accepted")
        check(service.selectedRecipients.length===1 && service.selectedRecipients[0]===duplicate,"Wrong exact identity selected")
        check(composer.text==="@Codex (isolated)[bbbbbbbbbbbb] ","Readable token missing")
        var request=service.prepareSubmission()
        check(request.mentions.length===1 && request.mentions[0]===duplicate,"Wire mention missing/wrong")
        check(!view.mentionOpen,"Picker active during send")
        service.applyDelivery({state:"acknowledged",requestId:request.id,roomId:room,eventId:"d".repeat(64),category:null})
        check(service.draftText==="" && service.selectedRecipients.length===0,"Accepted inline mention leaked into next message")
        type("@cla")
        check(view.mentionMatches.length===1 && view.chooseMention(0),"Claude completion failed")
        type("ordinary text")
        check(service.selectedRecipients.length===0,"Deleted inline mention still notifies")
        service.toggleRecipient(codex)
        type("@cod")
        check(view.chooseMention(0),"Already manually selected recipient rejected")
        type("ordinary text")
        check(service.selectedRecipients.length===1 && service.selectedRecipients[0]===codex,"Text editing removed explicit manual selection")
        type("person@cod")
        check(!view.mentionOpen && service.insertMention(codex,6,10)===-1,"Email treated as mention")
        type("@unknown")
        check(!view.mentionOpen,"Unknown agent invented")
        type("@cla")
        service.recipientsRoomId=other
        check(!view.mentionOpen && service.insertMention(claude,0,4)===-1,"Foreign roster reused")
        service.recipientsRoomId=room
        check(view.chooseMention(0),"Roster restoration failed")
        service.selectedRoomId=other
        check(service.selectedRecipients.length===0,"Mention leaked into another room")
        service.selectedRoomId=room
        type("@claudeExtra")
        check(service.selectedRecipients.indexOf(claude)===-1,"Mention prefix retained removed token identity")
        console.log("PASS: rendered @ completion, duplicate identities, exact wire keys, deletion, acknowledgement cleanup and room boundaries")
        Qt.quit()
      } catch(e) { console.error(e); Qt.exit(1) }
    }
  }
}
