// Actual composer and exact mention intent; no helper, relay or model.
import QtQuick
import Quickshell
import "plugin" as Buzz
ShellRoot {
  id: test
  property alias testedService: service
  Buzz.Service { id: service; autoConnect: false }
  Buzz.PanelContent { id: view; width: 1100; height: 1000; service: test.testedService }
  function findComposer(item) { return findNamed(item, "buzzComposer") }
  function findNamed(item, name) {
    if (item.objectName === name) return item
    for (var i=0;i<item.children.length;i++) { var found=findNamed(item.children[i], name); if(found) return found }
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
        // Hand-typed names resolve on send only when they can mean one entry.
        service.toggleRecipient(codex)
        check(service.selectedRecipients.length===0,"Explicit selection not cleared")
        var vclaude="e".repeat(64), spaced="f".repeat(64)
        service.recipientEntries=[{key:codex,name:"Codex (isolated)"},{key:duplicate,name:"Codex (isolated)"},{key:claude,name:"Claude"},{key:vclaude,name:"vClaude"},{key:spaced,name:"Claude Code"}]
        var caption=findNamed(view, "buzzComposerNotifies")
        check(caption!==null,"Notifies caption missing")
        function typed(text, keys, why) {
          type(text)
          check(JSON.stringify(service.outgoingMentions)===JSON.stringify(keys),why+": "+JSON.stringify(service.outgoingMentions))
        }
        typed("@vclaude are you up?",[vclaude],"Exact hand-typed name not resolved")
        check(caption.visible && caption.text==="Notifies: vClaude","Caption not shown before send: "+caption.text)
        check(service.selectedRecipients.length===0,"Resolution changed the explicit selection")
        request=service.prepareSubmission()
        check(request.mentions.length===1 && request.mentions[0]===vclaude,"Resolved key not sent")
        service.applyDelivery({state:"acknowledged",requestId:request.id,roomId:room,eventId:"e".repeat(64),category:null})
        check(service.draftText==="" && service.outgoingMentions.length===0 && !caption.visible,"Resolution outlived its message")
        typed("hi @VCLAUDE.",[vclaude],"Case difference not resolved")
        typed("@Codex (isolated) look",[],"Ambiguous name resolved")
        check(!caption.visible,"Caption shown without mentions")
        typed("@Claude Code please",[spaced],"Name with spaces not resolved as the longest name")
        typed("@claude-code and @claudecode",[spaced],"Dashed or joined name not resolved")
        typed("@Claude codex",[claude],"Shorter exact name not resolved")
        typed("@claudeExtra person@vclaude @vclaude's",[],"Partial word or email resolved")
        typed("@vclaude @Claude",[vclaude,claude],"Two names not both resolved")
        check(caption.text==="Notifies: vClaude, Claude","Caption does not list every name: "+caption.text)
        service.toggleRecipient(vclaude)
        typed("@vClaude hi",[vclaude],"Already-selected key duplicated")
        request=service.prepareSubmission()
        check(request.mentions.length===1 && request.mentions[0]===vclaude,"Duplicate wire mention")
        service.applyDelivery({state:"acknowledged",requestId:request.id,roomId:room,eventId:"f".repeat(64),category:null})
        // An explicit choice outlives the message, as before; clear it here.
        service.toggleRecipient(vclaude)
        check(service.selectedRecipients.length===0,"Explicit choice not cleared")
        service.recipientsRoomId=other
        typed("@vclaude hi",[],"Roster of another room used")
        service.recipientsRoomId=room
        service.recipientsState="loading"
        typed("@vclaude hi",[],"Roster that is not a snapshot used")
        service.recipientsState="snapshot"
        // The 20-key limit covers explicit and resolved keys together.
        var many=[]
        for (var n=10;n<30;n++) many.push({key:String(n).repeat(32),name:"P"+n})
        service.recipientEntries=service.recipientEntries.concat(many)
        type("")
        many.forEach(function(entry) { service.toggleRecipient(entry.key) })
        check(service.selectedRecipients.length===20,"Explicit selection limit changed")
        typed("@vclaude hi",many.map(function(entry) { return entry.key }),"Resolved key exceeded the limit")
        many.forEach(function(entry) { service.toggleRecipient(entry.key) })
        typed("@vclaude hi",[vclaude],"Resolution after the limit cleared")
        type("")
        console.log("PASS: rendered @ completion, duplicate identities, exact wire keys, deletion, acknowledgement cleanup, room boundaries and hand-typed name resolution")
        Qt.quit()
      } catch(e) { console.error(e); Qt.exit(1) }
    }
  }
}
